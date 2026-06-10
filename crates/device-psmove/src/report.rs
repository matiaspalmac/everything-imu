//! PS Move HID input report 0x01 parser.
//!
//! Layout reference: `docs/reference/psmove_protocol.md`. The 49-byte report packs
//! *two* IMU sub-frames, so each parse emits two [`ImuSample`]s.
//!
//! Byte offsets (0-indexed, report ID at byte 0):
//!
//! | Field            | frame A | frame B |
//! |------------------|---------|---------|
//! | accel X / Z / Y  | 13/15/17| 19/21/23|
//! | gyro  X / Z / Y  | 25/27/29| 31/33/35|
//! | magnetometer     | 38..42 (packed 12-bit, ZCM1 only)  |
//! | battery          | 12                                  |
//! | timestamp hi/lo  | 11 / 43                             |
//!
//! Each accel/gyro component is a **little-endian u16** (low byte first). On
//! **ZCM1** the value is biased by `0x8000` (subtract to centre); on **ZCM2**
//! it is plain two's-complement `i16` (no bias). Wire axis order is X-Z-Y and is
//! deswizzled to X-Y-Z via [`crate::axis_remap`].

use crate::axis_remap;
use crate::calibration::ImuCalibration;
use crate::ids::ControllerKind;
use device_traits::{BatteryState, ChannelInfo, ImuSample};
use std::time::Instant;

const G_TO_M_S2: f32 = 9.806_65;
const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

// Nominal uncalibrated scales (per `ref_psmove_protocol.md` §"IMU scale
// factors"). Factory calibration (feature report 0x10) refines these; VQF also
// normalises gravity during warm-up, so nominal values are sufficient for
// first-light fusion.
const ACCEL_LSB_PER_G: f32 = 16384.0;
const ACCEL_SCALE_M_S2: f32 = G_TO_M_S2 / ACCEL_LSB_PER_G;
const GYRO_LSB_PER_DEG_S: f32 = 16.4;
const GYRO_SCALE_RAD_S: f32 = DEG_TO_RAD / GYRO_LSB_PER_DEG_S;

// Frame offsets within the 49-byte report.
const OFS_TIMESTAMP_HI: usize = 11;
const OFS_BATTERY: usize = 12;
const OFS_FRAME_A_ACCEL: usize = 13;
const OFS_FRAME_B_ACCEL: usize = 19;
const OFS_FRAME_A_GYRO: usize = 25;
const OFS_FRAME_B_GYRO: usize = 31;
const OFS_MAG: usize = 38;
const OFS_TIMESTAMP_LO: usize = 43;
const REPORT_MIN_LEN: usize = 49;

// Nominal report period: ~175 reports/s (two sub-frames each). Used only to
// seed the dt estimator on the very first report, before a real interval is
// measured.
const NOMINAL_REPORT_DT_US: f32 = 1_000_000.0 / 175.0;

/// Monotonic timestamp source for the PS Move sample stream.
///
/// The hardware exposes only a 16-bit free-running counter (bytes 11/43) whose
/// absolute tick rate is undocumented and wraps every ~65 k ticks, so — per the
/// JC1 ice-skating ratefix lesson — fusion is driven from a *measured* inter-
/// report interval (EMA-smoothed) rather than the raw counter. The two
/// sub-frames in each report are spaced half an interval apart.
#[derive(Debug)]
pub struct ReportClock {
    last: Option<Instant>,
    accum_us: u64,
    ema_dt_us: f32,
}

impl Default for ReportClock {
    fn default() -> Self {
        Self {
            last: None,
            accum_us: 0,
            ema_dt_us: NOMINAL_REPORT_DT_US,
        }
    }
}

impl ReportClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the clock by one report arriving at `now`, returning the
    /// timestamps (µs) for sub-frame A (earlier) and sub-frame B (later).
    fn tick(&mut self, now: Instant) -> (u64, u64) {
        let dt_us = match self.last {
            Some(prev) => now.duration_since(prev).as_micros() as f32,
            None => NOMINAL_REPORT_DT_US,
        };
        self.last = Some(now);
        // EMA (α=0.1) tames USB/BT scheduling jitter the way the JC1 fix does.
        self.ema_dt_us = self.ema_dt_us * 0.9 + dt_us * 0.1;
        let dt = self.ema_dt_us.max(1.0) as u64;
        let frame_a = self.accum_us + dt / 2;
        let frame_b = self.accum_us + dt;
        self.accum_us = frame_b;
        (frame_a, frame_b)
    }
}

fn read_u16_le(buf: &[u8], offset: usize) -> Option<u16> {
    let bytes = buf.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Centre a raw accel/gyro component. ZCM1 biases by `0x8000`; ZCM2 is plain
/// two's-complement.
fn centered(kind: ControllerKind, raw: u16) -> i32 {
    match kind {
        ControllerKind::Zcm1 => raw as i32 - 0x8000,
        ControllerKind::Zcm2 => raw as i16 as i32,
    }
}

/// Sign-extend a 12-bit magnetometer field (range -2048..=2047).
fn sign_extend_12(v: u16) -> i32 {
    let v = (v & 0x0FFF) as i32;
    if v & 0x0800 != 0 {
        v - 0x1000
    } else {
        v
    }
}

/// Read a centred, deswizzled raw-LSB triplet (X-Y-Z), pre-calibration.
fn raw_triplet(kind: ControllerKind, buf: &[u8], ofs: usize) -> Option<[f32; 3]> {
    let a = centered(kind, read_u16_le(buf, ofs)?);
    let b = centered(kind, read_u16_le(buf, ofs + 2)?);
    let c = centered(kind, read_u16_le(buf, ofs + 4)?);
    // Wire order is X-Z-Y; deswizzle to X-Y-Z (still raw LSB).
    Some(axis_remap::deswizzle(kind, [a as f32, b as f32, c as f32]))
}

#[inline]
fn scale3(v: [f32; 3], scale: f32) -> [f32; 3] {
    [v[0] * scale, v[1] * scale, v[2] * scale]
}

fn parse_frame(
    kind: ControllerKind,
    buf: &[u8],
    accel_ofs: usize,
    gyro_ofs: usize,
    timestamp_us: u64,
    cal: &ImuCalibration,
) -> Option<ImuSample> {
    // Factory calibration is applied on raw LSB (in deswizzled X-Y-Z order),
    // then the nominal scale converts to m/s² and rad/s.
    let accel = scale3(
        cal.apply_accel(raw_triplet(kind, buf, accel_ofs)?),
        ACCEL_SCALE_M_S2,
    );
    let gyro = scale3(
        cal.apply_gyro(raw_triplet(kind, buf, gyro_ofs)?),
        GYRO_SCALE_RAD_S,
    );
    Some(ImuSample {
        gyro,
        accel,
        mag: None,
        timestamp_us,
    })
}

/// Unpack the ZCM1 packed 12-bit magnetometer (bytes 38..42), deswizzled to
/// X-Y-Z. Returns raw signed LSB — magnetometer hard/soft-iron calibration
/// (fig-8) lives in [`crate::calibration`].
fn parse_mag(kind: ControllerKind, buf: &[u8]) -> Option<[f32; 3]> {
    let b = buf.get(OFS_MAG..OFS_MAG + 5)?;
    // Packed layout (per `docs/reference/psmove_protocol.md`): X = lo-nibble(b0):b1,
    // Z = b2:hi-nibble(b3), Y = lo-nibble(b3):b4. Wire order X-Z-Y like accel/gyro.
    let x = sign_extend_12((((b[0] & 0x0F) as u16) << 8) | b[1] as u16);
    let z = sign_extend_12(((b[2] as u16) << 4) | ((b[3] >> 4) as u16));
    let y = sign_extend_12((((b[3] & 0x0F) as u16) << 8) | b[4] as u16);
    Some(axis_remap::deswizzle(kind, [x as f32, z as f32, y as f32]))
}

/// Parse a PS Move HID input report 0x01. Emits one `ImuSamples` event (two
/// sub-frames) and a `Battery` event when the byte is recognised. Returns false
/// on short / unparseable reports so the caller can log.
pub fn parse_report(
    kind: ControllerKind,
    buf: &[u8],
    clock: &mut ReportClock,
    cal: &ImuCalibration,
    out: &tokio::sync::mpsc::Sender<ChannelInfo>,
) -> bool {
    if buf.len() < REPORT_MIN_LEN {
        return false;
    }

    let (ts_a, ts_b) = clock.tick(Instant::now());

    let mag = if kind.has_magnetometer() {
        parse_mag(kind, buf)
    } else {
        None
    };

    let mut samples = Vec::with_capacity(2);
    if let Some(mut s) = parse_frame(kind, buf, OFS_FRAME_A_ACCEL, OFS_FRAME_A_GYRO, ts_a, cal) {
        s.mag = mag;
        samples.push(s);
    }
    if let Some(mut s) = parse_frame(kind, buf, OFS_FRAME_B_ACCEL, OFS_FRAME_B_GYRO, ts_b, cal) {
        s.mag = mag;
        samples.push(s);
    }

    if samples.is_empty() {
        return false;
    }

    let _ = out.try_send(ChannelInfo::ImuSamples(samples));

    if let Some(byte) = buf.get(OFS_BATTERY).copied() {
        // PS Move battery byte: 0x00..0x05 discharging levels, 0xEE = charging,
        // 0xEF = charged. Unknown values are skipped — NaN would leak into UI
        // and fusion downstream.
        let parsed = match byte {
            0x00 => Some((0.0, false)),
            0x01 => Some((0.2, false)),
            0x02 => Some((0.4, false)),
            0x03 => Some((0.6, false)),
            0x04 => Some((0.8, false)),
            0x05 => Some((1.0, false)),
            0xEE => Some((0.5, true)),
            0xEF => Some((1.0, true)),
            _ => None,
        };
        if let Some((fraction, charging)) = parsed {
            let _ = out.try_send(ChannelInfo::Battery(BatteryState { fraction, charging }));
        }
    }

    let _ = (OFS_TIMESTAMP_HI, OFS_TIMESTAMP_LO);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    /// Write a centred accel/gyro component for `kind` into `buf` at `ofs` as
    /// little-endian: encodes `value` such that `centered()` recovers it.
    fn put_centered(buf: &mut [u8], ofs: usize, kind: ControllerKind, value: i32) {
        let raw: u16 = match kind {
            ControllerKind::Zcm1 => (value + 0x8000) as u16,
            ControllerKind::Zcm2 => value as i16 as u16,
        };
        buf[ofs..ofs + 2].copy_from_slice(&raw.to_le_bytes());
    }

    /// Encode the centred-zero value (ZCM1: 0x8000, ZCM2: 0x0000) into every
    /// accel/gyro slot of both sub-frames, so unset slots decode to 0 rather
    /// than ZCM1's `0 - 0x8000 = -32768`.
    fn zero_imu_slots(buf: &mut [u8], kind: ControllerKind) {
        for ofs in [
            OFS_FRAME_A_ACCEL,
            OFS_FRAME_A_GYRO,
            OFS_FRAME_B_ACCEL,
            OFS_FRAME_B_GYRO,
        ] {
            for slot in 0..3 {
                put_centered(buf, ofs + slot * 2, kind, 0);
            }
        }
    }

    #[test]
    fn zcm1_report_emits_two_imu_samples_deswizzled() {
        let mut buf = vec![0u8; 49];
        let kind = ControllerKind::Zcm1;
        zero_imu_slots(&mut buf, kind);
        buf[OFS_BATTERY] = 0x05;
        // deswizzle maps wire [s0,s1,s2] (X-Z-Y) → [s0,s2,s1] (X-Y-Z): the wire
        // Y slot (offset+4) lands on pipeline Y. Put +1 g there.
        put_centered(&mut buf, OFS_FRAME_A_ACCEL + 4, kind, 16384);
        // Frame A gyro wire X slot (offset+0) → pipeline X; ~10 dps.
        put_centered(&mut buf, OFS_FRAME_A_GYRO, kind, 164);
        // Frame B accel: +1 g on the wire Z slot (offset+2) → pipeline Z.
        put_centered(&mut buf, OFS_FRAME_B_ACCEL + 2, kind, 16384);

        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        let mut clock = ReportClock::new();
        assert!(parse_report(
            kind,
            &buf,
            &mut clock,
            &ImuCalibration::identity(),
            &tx
        ));

        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(samples) => {
                assert_eq!(samples.len(), 2);
                // Wire Y slot carried +1g → deswizzled onto pipeline Y.
                assert!((samples[0].accel[1] - 9.806).abs() < 0.05);
                assert!(samples[0].accel[0].abs() < 0.05 && samples[0].accel[2].abs() < 0.05);
                // Gyro X ~ 10 dps in rad/s.
                assert!((samples[0].gyro[0] - 10.0_f32.to_radians()).abs() < 0.01);
                // Frame B: wire Z slot → pipeline Z.
                assert!((samples[1].accel[2] - 9.806).abs() < 0.05);
                // Monotonic timestamps, A before B.
                assert!(samples[1].timestamp_us > samples[0].timestamp_us);
            }
            other => panic!("expected ImuSamples, got {other:?}"),
        }

        match rx.try_recv().expect("battery event") {
            ChannelInfo::Battery(b) => assert!((b.fraction - 1.0).abs() < f32::EPSILON),
            other => panic!("expected Battery, got {other:?}"),
        }
    }

    #[test]
    fn zcm2_uses_twos_complement_and_drops_mag() {
        let mut buf = vec![0u8; 49];
        let kind = ControllerKind::Zcm2;
        // -16384 in two's complement on wire Y slot → -1g on pipeline Y.
        put_centered(&mut buf, OFS_FRAME_A_ACCEL + 4, kind, -16384);
        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        let mut clock = ReportClock::new();
        assert!(parse_report(
            kind,
            &buf,
            &mut clock,
            &ImuCalibration::identity(),
            &tx
        ));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(samples) => {
                assert!(samples[0].mag.is_none());
                assert!((samples[0].accel[1] + 9.806).abs() < 0.05);
            }
            other => panic!("expected ImuSamples, got {other:?}"),
        }
    }

    #[test]
    fn zcm1_magnetometer_unpacks_12bit_and_deswizzles() {
        let mut buf = vec![0u8; 49];
        let kind = ControllerKind::Zcm1;
        // Encode wire mag X=1, Z=2, Y=3 in the packed 12-bit layout.
        let (mx, mz, my) = (1u16, 2u16, 3u16);
        buf[OFS_MAG] = ((mx >> 8) & 0x0F) as u8;
        buf[OFS_MAG + 1] = (mx & 0xFF) as u8;
        buf[OFS_MAG + 2] = ((mz >> 4) & 0xFF) as u8;
        buf[OFS_MAG + 3] = (((mz & 0x0F) << 4) as u8) | ((my >> 8) & 0x0F) as u8;
        buf[OFS_MAG + 4] = (my & 0xFF) as u8;
        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        let mut clock = ReportClock::new();
        assert!(parse_report(
            kind,
            &buf,
            &mut clock,
            &ImuCalibration::identity(),
            &tx
        ));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(samples) => {
                // Wire X-Z-Y (1,2,3) deswizzles to X-Y-Z (1,3,2).
                assert_eq!(samples[0].mag, Some([1.0, 3.0, 2.0]));
            }
            other => panic!("expected ImuSamples, got {other:?}"),
        }
    }

    #[test]
    fn short_report_rejected() {
        let buf = vec![0u8; 16];
        let (tx, _rx) = mpsc::channel::<ChannelInfo>(8);
        let mut clock = ReportClock::new();
        assert!(!parse_report(
            ControllerKind::Zcm1,
            &buf,
            &mut clock,
            &ImuCalibration::identity(),
            &tx
        ));
    }

    #[test]
    fn sign_extend_12_handles_negatives() {
        assert_eq!(sign_extend_12(0x000), 0);
        assert_eq!(sign_extend_12(0x7FF), 2047);
        assert_eq!(sign_extend_12(0x800), -2048);
        assert_eq!(sign_extend_12(0xFFF), -1);
    }
}

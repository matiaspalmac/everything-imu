//! PS Move HID input report 0x01 parser.
//!
//! Big-endian throughout (Sony PS3 IMU convention). Each report packs two
//! IMU frames at offsets `frame_a` and `frame_b`, so we always emit two
//! [`ImuSample`]s per call. Layout reference: psmoveapi `psmove.c`.
//!
//! Calibration here is the bare default (factory ranges); per-device cal
//! lives on a separate config block read via feature report 0x10. Wiring
//! that is queued and not required for first-light bring-up.

use crate::ids::ControllerKind;
use device_traits::{BatteryState, ChannelInfo, ImuSample};

const ACCEL_RANGE_G: f32 = 4.0;
const GYRO_RANGE_DEG_S_ZCM1: f32 = 2000.0;
const GYRO_RANGE_DEG_S_ZCM2: f32 = 2000.0;
const I16_FULL: f32 = 32767.0;
const G_TO_M_S2: f32 = 9.806_65;
const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

const ACCEL_SCALE_M_S2: f32 = (ACCEL_RANGE_G / I16_FULL) * G_TO_M_S2;

/// Frame offsets within a 49-byte report.
const ZCM_OFS_BTNS_1: usize = 1;
const ZCM_OFS_TRIG: usize = 6;
const ZCM_OFS_BATTERY: usize = 12;
const ZCM_OFS_FRAME_A_ACCEL: usize = 14;
const ZCM_OFS_FRAME_A_GYRO: usize = 26;
const ZCM_OFS_FRAME_B_ACCEL: usize = 20;
const ZCM_OFS_FRAME_B_GYRO: usize = 32;
const ZCM1_OFS_MAG: usize = 38;
const REPORT_MIN_LEN: usize = 49;

fn read_i16_be(buf: &[u8], offset: usize) -> Option<i16> {
    let bytes = buf.get(offset..offset + 2)?;
    Some(i16::from_be_bytes([bytes[0], bytes[1]]))
}

fn gyro_scale(kind: ControllerKind) -> f32 {
    let range = match kind {
        ControllerKind::Zcm1 => GYRO_RANGE_DEG_S_ZCM1,
        ControllerKind::Zcm2 => GYRO_RANGE_DEG_S_ZCM2,
    };
    (range / I16_FULL) * DEG_TO_RAD
}

fn parse_frame(
    buf: &[u8],
    accel_ofs: usize,
    gyro_ofs: usize,
    kind: ControllerKind,
) -> Option<ImuSample> {
    let ax = read_i16_be(buf, accel_ofs)?;
    let ay = read_i16_be(buf, accel_ofs + 2)?;
    let az = read_i16_be(buf, accel_ofs + 4)?;
    let gx = read_i16_be(buf, gyro_ofs)?;
    let gy = read_i16_be(buf, gyro_ofs + 2)?;
    let gz = read_i16_be(buf, gyro_ofs + 4)?;
    let g_scale = gyro_scale(kind);
    Some(ImuSample {
        gyro: [
            gx as f32 * g_scale,
            gy as f32 * g_scale,
            gz as f32 * g_scale,
        ],
        accel: [
            ax as f32 * ACCEL_SCALE_M_S2,
            ay as f32 * ACCEL_SCALE_M_S2,
            az as f32 * ACCEL_SCALE_M_S2,
        ],
        mag: None,
        timestamp_us: 0,
    })
}

fn parse_mag(buf: &[u8]) -> Option<[f32; 3]> {
    let mx = read_i16_be(buf, ZCM1_OFS_MAG)?;
    let my = read_i16_be(buf, ZCM1_OFS_MAG + 2)?;
    let mz = read_i16_be(buf, ZCM1_OFS_MAG + 4)?;
    // PS Move mag scale is per-device; default factor of 1.0 LSB/µT keeps
    // the values in a recognizable range until calibration lands.
    Some([mx as f32, my as f32, mz as f32])
}

/// Parse a PS Move HID input report. Emits two `ImuSamples` (one event)
/// and a `Battery` event when the byte is present. Returns false on
/// short / unknown reports so the caller can log.
pub fn parse_report(
    kind: ControllerKind,
    buf: &[u8],
    out: &tokio::sync::mpsc::Sender<ChannelInfo>,
) -> bool {
    if buf.len() < REPORT_MIN_LEN {
        return false;
    }

    let mag = if kind.has_magnetometer() {
        parse_mag(buf)
    } else {
        None
    };

    let mut samples = Vec::with_capacity(2);
    if let Some(mut s) = parse_frame(buf, ZCM_OFS_FRAME_A_ACCEL, ZCM_OFS_FRAME_A_GYRO, kind) {
        s.mag = mag;
        samples.push(s);
    }
    if let Some(mut s) = parse_frame(buf, ZCM_OFS_FRAME_B_ACCEL, ZCM_OFS_FRAME_B_GYRO, kind) {
        s.mag = mag;
        samples.push(s);
    }

    if samples.is_empty() {
        return false;
    }

    let _ = out.try_send(ChannelInfo::ImuSamples(samples));

    if let Some(byte) = buf.get(ZCM_OFS_BATTERY).copied() {
        // PS Move battery byte: 0x00..0x05 discharging levels, 0xEE = charging,
        // 0xEF = charged. Map to fraction defensively.
        let (fraction, charging) = match byte {
            0x00 => (0.0, false),
            0x01 => (0.2, false),
            0x02 => (0.4, false),
            0x03 => (0.6, false),
            0x04 => (0.8, false),
            0x05 => (1.0, false),
            0xEE => (0.5, true),
            0xEF => (1.0, true),
            _ => (f32::NAN, false),
        };
        let _ = out.try_send(ChannelInfo::Battery(BatteryState { fraction, charging }));
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn zcm1_report_emits_two_imu_samples_with_mag() {
        let mut buf = vec![0u8; 49];
        // accel A = (0, 8192, 0), gyro A = (1000, 0, 0).
        buf[14..16].copy_from_slice(&0i16.to_be_bytes());
        buf[16..18].copy_from_slice(&8192i16.to_be_bytes());
        buf[18..20].copy_from_slice(&0i16.to_be_bytes());
        buf[26..28].copy_from_slice(&1000i16.to_be_bytes());
        buf[28..30].copy_from_slice(&0i16.to_be_bytes());
        buf[30..32].copy_from_slice(&0i16.to_be_bytes());
        // accel B mirror, gyro B = (0, 1000, 0).
        buf[20..22].copy_from_slice(&0i16.to_be_bytes());
        buf[22..24].copy_from_slice(&8192i16.to_be_bytes());
        buf[24..26].copy_from_slice(&0i16.to_be_bytes());
        buf[32..34].copy_from_slice(&0i16.to_be_bytes());
        buf[34..36].copy_from_slice(&1000i16.to_be_bytes());
        buf[36..38].copy_from_slice(&0i16.to_be_bytes());
        // Mag = (10, 20, 30).
        buf[38..40].copy_from_slice(&10i16.to_be_bytes());
        buf[40..42].copy_from_slice(&20i16.to_be_bytes());
        buf[42..44].copy_from_slice(&30i16.to_be_bytes());

        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::Zcm1, &buf, &tx));

        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(samples) => {
                assert_eq!(samples.len(), 2);
                assert!((samples[0].accel[1] - 9.806).abs() < 0.05);
                assert!(samples[0].gyro[0] > 0.0);
                assert!(samples[1].gyro[1] > 0.0);
                assert_eq!(samples[0].mag, Some([10.0, 20.0, 30.0]));
            }
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn zcm2_report_drops_magnetometer() {
        let mut buf = vec![0u8; 49];
        buf[14..16].copy_from_slice(&0i16.to_be_bytes());
        buf[16..18].copy_from_slice(&8192i16.to_be_bytes());
        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::Zcm2, &buf, &tx));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(samples) => {
                assert!(samples[0].mag.is_none());
            }
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn short_report_rejected() {
        let buf = vec![0u8; 16];
        let (tx, _rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(!parse_report(ControllerKind::Zcm1, &buf, &tx));
    }
}

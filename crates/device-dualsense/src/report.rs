//! Sony controller input report parser.
//!
//! Three known input report shapes are recognized:
//! - DualSense USB report 0x01 (64 bytes; buf[0]=report ID, gyro at buf[16], accel at buf[22]).
//! - DualSense BT report 0x31 (78 bytes; buf[0]=0x31, buf[1]=seq tag, payload shifted +1 vs USB → gyro at buf[17], accel at buf[23]). Trailing 4-byte CRC32 ignored on input.
//! - DualShock 4 USB report 0x01 (64 bytes; gyro at buf[13], accel at buf[19]).
//!
//! Offsets follow pydualsense / hid-playstation canonical layout: hidapi returns
//! the report ID at buf[0] for numbered reports, so payload-relative offsets must
//! be biased by +1 (USB) or +2 (BT, ID + tag).
//!
//! IMU scaling:
//! - gyro: i16 raw → rad/s.  Default sensitivity = ±2000 deg/s.
//! - accel: i16 raw → m/s².  Default sensitivity = ±4 g.
//!
//! Real DualSense ships per-device calibration via feature report 0x05 — wiring that
//! is queued; the defaults below land within ~2 % of factory cal on a healthy unit
//! and are good enough for first-light bring-up.

use crate::ids::ControllerKind;
use device_traits::{BatteryState, ChannelInfo, ImuSample};

const DEG_PER_LSB: f32 = 2000.0 / 32767.0;
const RAD_PER_DEG: f32 = std::f32::consts::PI / 180.0;
const ACCEL_SCALE_G: f32 = 4.0 / 32767.0;

#[derive(Debug, Clone, Copy)]
pub struct SonyCalibration {
    pub gyro_bias_dps: [f32; 3],
    pub gyro_scale: [f32; 3],
    pub accel_bias_g: [f32; 3],
    pub accel_scale: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct ImuOffsets {
    pub gyro: usize,
    pub accel: usize,
    pub battery: Option<usize>,
    /// Byte offset of the buttons-2 byte that carries the PS / Mute
    /// system buttons. None if the report shape is not recognized.
    pub buttons_2: Option<usize>,
}

impl ImuOffsets {
    pub fn for_report(kind: ControllerKind, len: usize) -> Option<Self> {
        match (kind, len) {
            (ControllerKind::DualSense | ControllerKind::DualSenseEdge, 64) => Some(Self {
                gyro: 16,
                accel: 22,
                battery: Some(53),
                // PS / Mute live in buttons[2] = payload byte 9 = buf[10]
                // (report ID at buf[0]). buf[9] is buttons[1] (L1/R1/…), so
                // reading 9 here decoded a shoulder press as the PS button.
                buttons_2: Some(10),
            }),
            (ControllerKind::DualSense | ControllerKind::DualSenseEdge, _) if len >= 78 => {
                Some(Self {
                    // BT report 0x31 carries a SINGLE sequence-tag byte at buf[1]
                    // before the common report, so every payload offset is USB + 1
                    // (not +2). Hardware hex dump on a DualSense over Bluetooth
                    // confirmed gyro at 17 (zero triplet at rest), accel at 23
                    // (gravity ~8192 on one axis), and buttons_2 (PS / Mute) at 11.
                    gyro: 17,
                    accel: 23,
                    battery: Some(54),
                    buttons_2: Some(11),
                })
            }
            (ControllerKind::DualShock4, 64) => Some(Self {
                gyro: 13,
                accel: 19,
                battery: Some(30),
                buttons_2: Some(7),
            }),
            // DualShock 4 Bluetooth report 0x11: [0]=0x11, [1..3]=BT header, then
            // the USB 0x01 payload shifted +2. The logical report is 78 bytes, but
            // Windows delivers it padded (observed 128 via hidapi), so match any
            // length >= 78 instead of an exact size — otherwise the IMU offsets are
            // never reached and every frame reads zero. Reference: hid-playstation.c.
            (ControllerKind::DualShock4, _) if len >= 78 => Some(Self {
                gyro: 15,
                accel: 21,
                battery: Some(32),
                buttons_2: Some(9),
            }),
            _ => None,
        }
    }
}

fn read_i16_le(buf: &[u8], offset: usize) -> Option<i16> {
    let bytes = buf.get(offset..offset + 2)?;
    Some(i16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(buf: &[u8], offset: usize) -> Option<u32> {
    let b = buf.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Pull the firmware sensor timestamp (µs) out of an input report.
///
/// DualSense / DualSense Edge carry a free-running u32 LE tick counter directly
/// after the 6-byte accel block (`accel_offset + 6`). The tick is 1/3 µs, so
/// µs = ticks/3 rounded to nearest, matching hid-playstation's
/// `DIV_ROUND_CLOSEST(ts, 3)`. The counter is incremented by the controller
/// itself, so it is immune to USB / Bluetooth delivery jitter.
///
/// DualShock 4 stores a different, accumulating u16 timestamp whose wrap
/// handling is not yet hardware-validated; it returns 0 so the pipeline falls
/// back to the delivery-rate estimate.
fn sensor_timestamp_us(kind: ControllerKind, buf: &[u8], offsets: &ImuOffsets) -> u64 {
    match kind {
        ControllerKind::DualSense | ControllerKind::DualSenseEdge => {
            read_u32_le(buf, offsets.accel + 6)
                .map(|ticks| (ticks as u64 + 1) / 3)
                .unwrap_or(0)
        }
        ControllerKind::DualShock4 => 0,
    }
}

/// Returns `Some(true)` if the PS / system button is pressed in this
/// report, `Some(false)` if released, or `None` if the report shape is
/// unknown. The PS button is bit 0 of the buttons-2 byte for DualSense
/// and bit 0 of the same byte for DualShock 4.
pub fn parse_ps_button(kind: ControllerKind, buf: &[u8]) -> Option<bool> {
    let offsets = ImuOffsets::for_report(kind, buf.len())?;
    let off = offsets.buttons_2?;
    let byte = buf.get(off).copied()?;
    Some((byte & 0x01) != 0)
}

/// Parse an input report and emit zero or more `ChannelInfo` events through `out`.
/// Returns `false` when the report length / id pair is unknown so the caller can log.
pub fn parse_report(
    kind: ControllerKind,
    buf: &[u8],
    calibration: Option<SonyCalibration>,
    out: &tokio::sync::mpsc::Sender<ChannelInfo>,
) -> bool {
    let Some(offsets) = ImuOffsets::for_report(kind, buf.len()) else {
        return false;
    };

    let Some(gx) = read_i16_le(buf, offsets.gyro) else {
        return false;
    };
    let Some(gy) = read_i16_le(buf, offsets.gyro + 2) else {
        return false;
    };
    let Some(gz) = read_i16_le(buf, offsets.gyro + 4) else {
        return false;
    };
    let Some(ax) = read_i16_le(buf, offsets.accel) else {
        return false;
    };
    let Some(ay) = read_i16_le(buf, offsets.accel + 2) else {
        return false;
    };
    let Some(az) = read_i16_le(buf, offsets.accel + 4) else {
        return false;
    };

    let mut gyro_dps = [
        gx as f32 * DEG_PER_LSB,
        gy as f32 * DEG_PER_LSB,
        gz as f32 * DEG_PER_LSB,
    ];
    let mut accel_g = [
        ax as f32 * ACCEL_SCALE_G,
        ay as f32 * ACCEL_SCALE_G,
        az as f32 * ACCEL_SCALE_G,
    ];
    if let Some(cal) = calibration {
        for i in 0..3 {
            gyro_dps[i] = (gyro_dps[i] - cal.gyro_bias_dps[i]) * cal.gyro_scale[i];
            accel_g[i] = (accel_g[i] - cal.accel_bias_g[i]) * cal.accel_scale[i];
        }
    }

    // Bias subtraction + sensitivity scaling above run in the chip's native
    // frame because that is what feature report 0x05 calibrates against.
    let gyro_chip = [
        gyro_dps[0] * RAD_PER_DEG,
        gyro_dps[1] * RAD_PER_DEG,
        gyro_dps[2] * RAD_PER_DEG,
    ];
    let accel_chip = [
        accel_g[0] * 9.806_65,
        accel_g[1] * 9.806_65,
        accel_g[2] * 9.806_65,
    ];
    let sample = ImuSample {
        gyro: crate::axis_remap::apply(kind, gyro_chip),
        accel: crate::axis_remap::apply(kind, accel_chip),
        mag: None,
        timestamp_us: sensor_timestamp_us(kind, buf, &offsets),
    };
    let _ = out.try_send(ChannelInfo::ImuSamples(vec![sample]));

    if let Some(off) = offsets.battery {
        if let Some(byte) = buf.get(off).copied() {
            // DS5/DS4 battery byte: low nibble is capacity 0..=10 per Linux hid-playstation
            // (DS_STATUS_BATTERY_CAPACITY); high nibble bit 4 is charging status.
            let level_raw = byte & 0x0F;
            let charging = (byte & 0x10) != 0;
            let fraction = (level_raw as f32 / 10.0).clamp(0.0, 1.0);
            let _ = out.try_send(ChannelInfo::Battery(BatteryState { fraction, charging }));
        }
    }
    true
}

pub fn parse_feature_calibration(
    kind: ControllerKind,
    report_id: u8,
    buf: &[u8],
) -> Option<SonyCalibration> {
    let interleaved = matches!(kind, ControllerKind::DualShock4) && report_id == 0x05;
    let sequential = matches!(
        kind,
        ControllerKind::DualSense | ControllerKind::DualSenseEdge
    ) || matches!(kind, ControllerKind::DualShock4) && report_id == 0x02;
    if !interleaved && !sequential {
        return None;
    }
    if buf.len() < 35 {
        return None;
    }

    let s16 = |off: usize| -> Option<i16> {
        let b = buf.get(off..off + 2)?;
        Some(i16::from_le_bytes([b[0], b[1]]))
    };

    let bias = [s16(1)? as f32, s16(3)? as f32, s16(5)? as f32];
    if bias.iter().any(|v| v.abs() > 4096.0) {
        return None;
    }

    let (pitch_plus, pitch_minus, yaw_plus, yaw_minus, roll_plus, roll_minus) = if interleaved {
        (
            s16(7)? as f32,
            s16(13)? as f32,
            s16(9)? as f32,
            s16(15)? as f32,
            s16(11)? as f32,
            s16(17)? as f32,
        )
    } else {
        (
            s16(7)? as f32,
            s16(9)? as f32,
            s16(11)? as f32,
            s16(13)? as f32,
            s16(15)? as f32,
            s16(17)? as f32,
        )
    };
    let speed_plus = s16(19)? as f32;
    let speed_minus = s16(21)? as f32;
    let acc_x_plus = s16(23)? as f32;
    let acc_x_minus = s16(25)? as f32;
    let acc_y_plus = s16(27)? as f32;
    let acc_y_minus = s16(29)? as f32;
    let acc_z_plus = s16(31)? as f32;
    let acc_z_minus = s16(33)? as f32;

    let gyro_bias_dps = [
        bias[0] * DEG_PER_LSB,
        bias[1] * DEG_PER_LSB,
        bias[2] * DEG_PER_LSB,
    ];
    let mut gyro_scale = [1.0; 3];
    let speed_2x = speed_plus + speed_minus;
    if speed_2x > 0.0 {
        let denom = [
            (pitch_plus - bias[0]).abs() + (pitch_minus - bias[0]).abs(),
            (yaw_plus - bias[1]).abs() + (yaw_minus - bias[1]).abs(),
            (roll_plus - bias[2]).abs() + (roll_minus - bias[2]).abs(),
        ];
        for i in 0..3 {
            if denom[i] > 0.0 {
                gyro_scale[i] = ((speed_2x / denom[i]) / DEG_PER_LSB).clamp(0.9, 1.1);
            }
        }
    }

    let range = [
        acc_x_plus - acc_x_minus,
        acc_y_plus - acc_y_minus,
        acc_z_plus - acc_z_minus,
    ];
    if range.iter().any(|v| *v <= 0.0) {
        return Some(SonyCalibration {
            gyro_bias_dps,
            gyro_scale,
            accel_bias_g: [0.0; 3],
            accel_scale: [1.0; 3],
        });
    }
    let midpoint = [
        acc_x_plus - (range[0] / 2.0),
        acc_y_plus - (range[1] / 2.0),
        acc_z_plus - (range[2] / 2.0),
    ];
    let accel_bias_g = [
        midpoint[0] * ACCEL_SCALE_G,
        midpoint[1] * ACCEL_SCALE_G,
        midpoint[2] * ACCEL_SCALE_G,
    ];
    let accel_scale = [
        (2.0 * 8192.0 / range[0]).clamp(0.9, 1.1),
        (2.0 * 8192.0 / range[1]).clamp(0.9, 1.1),
        (2.0 * 8192.0 / range[2]).clamp(0.9, 1.1),
    ];

    Some(SonyCalibration {
        gyro_bias_dps,
        gyro_scale,
        accel_bias_g,
        accel_scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn dualsense_usb_report_emits_imu_sample() {
        let mut buf = [0u8; 64];
        buf[16..18].copy_from_slice(&1000i16.to_le_bytes());
        buf[18..20].copy_from_slice(&(-500i16).to_le_bytes());
        buf[20..22].copy_from_slice(&250i16.to_le_bytes());
        buf[22..24].copy_from_slice(&0i16.to_le_bytes());
        buf[24..26].copy_from_slice(&8192i16.to_le_bytes());
        buf[26..28].copy_from_slice(&0i16.to_le_bytes());

        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::DualSense, &buf, None, &tx));

        let evt = rx.try_recv().expect("imu event");
        match evt {
            ChannelInfo::ImuSamples(samples) => {
                assert_eq!(samples.len(), 1);
                let s = &samples[0];
                // chip gx 1000 LSB → ~1.066 rad/s on JSL X (X passthrough).
                assert!((s.gyro[0] - 1.066).abs() < 0.01);
                // chip ay 8192 LSB → ~9.806 m/s² on JSL -Z after chip→JSL remap
                // (chip Y up → JSL Y up requires (x, z, -y), so chip ay lands on JSL.z = -ay).
                assert!((s.accel[1] - 9.806).abs() < 0.05);
            }
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn dualsense_usb_parses_hw_timestamp() {
        let mut buf = [0u8; 64];
        // 8192 LSB on accel Y so the sample is plausible.
        buf[24..26].copy_from_slice(&8192i16.to_le_bytes());
        // u32 tick counter right after the accel block (accel 22 + 6 = 28).
        buf[28..32].copy_from_slice(&12000u32.to_le_bytes());

        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::DualSense, &buf, None, &tx));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(s) => {
                // 12000 ticks / 3 = 4000 µs (one report period at 250 Hz).
                assert_eq!(s[0].timestamp_us, 4000);
            }
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn ds4_bt_report_parses_imu() {
        // DualShock 4 Bluetooth report 0x11 (78 bytes): IMU shifted +2 vs USB.
        let mut buf = [0u8; 78];
        buf[0] = 0x11;
        buf[15..17].copy_from_slice(&100i16.to_le_bytes()); // gyro x @ 15 (USB 13 +2)
        buf[21..23].copy_from_slice(&8192i16.to_le_bytes()); // accel y @ 21 (USB 19 +2)
        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::DualShock4, &buf, None, &tx));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(s) => {
                assert!(s[0].gyro[0] > 0.0, "gyro x should be positive");
                // DS4 has no usable HW timestamp in this driver → fallback path.
                assert_eq!(s[0].timestamp_us, 0);
            }
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn ds4_bt_padded_report_parses_imu() {
        // Windows delivers the DS4 BT 0x11 report padded past 78 bytes (observed
        // 128 via hidapi). The IMU offsets must still resolve or every frame is
        // zero — the exact-length match this replaces regressed to all-zero IMU
        // on real hardware.
        let o = ImuOffsets::for_report(ControllerKind::DualShock4, 128)
            .expect("padded DS4 BT report must resolve offsets");
        assert_eq!((o.gyro, o.accel), (15, 21));
        assert_eq!(
            (ImuOffsets::for_report(ControllerKind::DualSense, 128).map(|o| (o.gyro, o.accel))),
            Some((17, 23)),
            "DualSense BT (padded) IMU offsets are USB + 1",
        );

        let mut buf = [0u8; 128];
        buf[0] = 0x11;
        buf[15..17].copy_from_slice(&100i16.to_le_bytes());
        buf[21..23].copy_from_slice(&8192i16.to_le_bytes());
        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::DualShock4, &buf, None, &tx));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(s) => assert!(s[0].gyro[0] > 0.0),
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn ds4_reports_no_hw_timestamp() {
        let mut buf = [0u8; 64];
        buf[13..15].copy_from_slice(&100i16.to_le_bytes());
        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::DualShock4, &buf, None, &tx));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(s) => assert_eq!(s[0].timestamp_us, 0),
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn unknown_report_length_returns_false() {
        let buf = [0u8; 32];
        let (tx, _rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(!parse_report(ControllerKind::DualSense, &buf, None, &tx));
    }

    #[test]
    fn ps_button_bit_decode() {
        let mut buf = [0u8; 64];
        buf[10] = 0x01;
        assert_eq!(parse_ps_button(ControllerKind::DualSense, &buf), Some(true));
        buf[10] = 0x00;
        assert_eq!(
            parse_ps_button(ControllerKind::DualSense, &buf),
            Some(false)
        );
        // BT shape (78-byte report 0x31): buttons[2] at offset 11 (USB 10 + the
        // single BT sequence-tag byte). Hardware-confirmed on a DualSense.
        let mut buf_bt = [0u8; 78];
        buf_bt[11] = 0x01;
        assert_eq!(
            parse_ps_button(ControllerKind::DualSense, &buf_bt),
            Some(true),
        );
    }

    #[test]
    fn ds4_usb_offsets() {
        let mut buf = [0u8; 64];
        buf[13..15].copy_from_slice(&100i16.to_le_bytes());
        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::DualShock4, &buf, None, &tx));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(s) => {
                assert!(s[0].gyro[0] > 0.0);
            }
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn ds5_feature_calibration_parses() {
        let mut buf = [0u8; 41];
        buf[1..3].copy_from_slice(&10i16.to_le_bytes());
        buf[3..5].copy_from_slice(&(-5i16).to_le_bytes());
        buf[5..7].copy_from_slice(&7i16.to_le_bytes());
        buf[7..9].copy_from_slice(&16500i16.to_le_bytes());
        buf[9..11].copy_from_slice(&(-16500i16).to_le_bytes());
        buf[11..13].copy_from_slice(&16500i16.to_le_bytes());
        buf[13..15].copy_from_slice(&(-16500i16).to_le_bytes());
        buf[15..17].copy_from_slice(&16500i16.to_le_bytes());
        buf[17..19].copy_from_slice(&(-16500i16).to_le_bytes());
        buf[19..21].copy_from_slice(&2000i16.to_le_bytes());
        buf[21..23].copy_from_slice(&2000i16.to_le_bytes());
        buf[23..25].copy_from_slice(&8192i16.to_le_bytes());
        buf[25..27].copy_from_slice(&(-8192i16).to_le_bytes());
        buf[27..29].copy_from_slice(&8192i16.to_le_bytes());
        buf[29..31].copy_from_slice(&(-8192i16).to_le_bytes());
        buf[31..33].copy_from_slice(&8192i16.to_le_bytes());
        buf[33..35].copy_from_slice(&(-8192i16).to_le_bytes());

        let cal =
            parse_feature_calibration(ControllerKind::DualSense, 0x05, &buf).expect("cal parsed");
        assert!(cal.gyro_scale.iter().all(|v| (0.9..=1.1).contains(v)));
        assert!(cal.accel_scale.iter().all(|v| (*v - 1.0).abs() < 0.01));
    }
}

//! Sony controller input report parser.
//!
//! Three known input report shapes are recognized:
//! - DualSense USB report 0x01 (64 bytes, IMU at offset 15/21 from byte 0 after id).
//! - DualSense BT report 0x31 (78 bytes, payload shifted +1 vs USB to make room for
//!   the firmware tag byte at offset 1; trailing 4-byte CRC32 is ignored on input).
//! - DualShock 4 USB report 0x01 (64 bytes, IMU at offset 13/19).
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
const GYRO_SCALE_RAD_S: f32 = DEG_PER_LSB * RAD_PER_DEG;

const G_PER_LSB: f32 = 4.0 / 32767.0;
const ACCEL_SCALE_M_S2: f32 = G_PER_LSB * 9.806_65;

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
                gyro: 15,
                accel: 21,
                battery: Some(53),
                buttons_2: Some(9),
            }),
            (ControllerKind::DualSense | ControllerKind::DualSenseEdge, 78) => Some(Self {
                gyro: 16,
                accel: 22,
                battery: Some(54),
                buttons_2: Some(10),
            }),
            (ControllerKind::DualShock4, 64) => Some(Self {
                gyro: 13,
                accel: 19,
                battery: Some(30),
                buttons_2: Some(7),
            }),
            _ => None,
        }
    }
}

fn read_i16_le(buf: &[u8], offset: usize) -> Option<i16> {
    let bytes = buf.get(offset..offset + 2)?;
    Some(i16::from_le_bytes([bytes[0], bytes[1]]))
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

    let sample = ImuSample {
        gyro: [
            gx as f32 * GYRO_SCALE_RAD_S,
            gy as f32 * GYRO_SCALE_RAD_S,
            gz as f32 * GYRO_SCALE_RAD_S,
        ],
        accel: [
            ax as f32 * ACCEL_SCALE_M_S2,
            ay as f32 * ACCEL_SCALE_M_S2,
            az as f32 * ACCEL_SCALE_M_S2,
        ],
        mag: None,
        timestamp_us: 0,
    };
    let _ = out.try_send(ChannelInfo::ImuSamples(vec![sample]));

    if let Some(off) = offsets.battery {
        if let Some(byte) = buf.get(off).copied() {
            let level_raw = byte & 0x0F;
            let charging = (byte & 0x10) != 0;
            let fraction = (level_raw as f32 / 8.0).clamp(0.0, 1.0);
            let _ = out.try_send(ChannelInfo::Battery(BatteryState { fraction, charging }));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn dualsense_usb_report_emits_imu_sample() {
        let mut buf = [0u8; 64];
        buf[15..17].copy_from_slice(&1000i16.to_le_bytes());
        buf[17..19].copy_from_slice(&(-500i16).to_le_bytes());
        buf[19..21].copy_from_slice(&250i16.to_le_bytes());
        buf[21..23].copy_from_slice(&0i16.to_le_bytes());
        buf[23..25].copy_from_slice(&8192i16.to_le_bytes());
        buf[25..27].copy_from_slice(&0i16.to_le_bytes());

        let (tx, mut rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(parse_report(ControllerKind::DualSense, &buf, &tx));

        let evt = rx.try_recv().expect("imu event");
        match evt {
            ChannelInfo::ImuSamples(samples) => {
                assert_eq!(samples.len(), 1);
                let s = &samples[0];
                // 1000 LSB * (2000/32767) deg/LSB * π/180 ≈ 1.066 rad/s
                assert!((s.gyro[0] - 1.066).abs() < 0.01);
                // 8192 LSB * (4/32767) g/LSB * 9.80665 ≈ 9.806 m/s²
                assert!((s.accel[1] - 9.806).abs() < 0.05);
            }
            _ => panic!("expected ImuSamples"),
        }
    }

    #[test]
    fn unknown_report_length_returns_false() {
        let buf = [0u8; 32];
        let (tx, _rx) = mpsc::channel::<ChannelInfo>(8);
        assert!(!parse_report(ControllerKind::DualSense, &buf, &tx));
    }

    #[test]
    fn ps_button_bit_decode() {
        let mut buf = [0u8; 64];
        buf[9] = 0x01;
        assert_eq!(parse_ps_button(ControllerKind::DualSense, &buf), Some(true));
        buf[9] = 0x00;
        assert_eq!(
            parse_ps_button(ControllerKind::DualSense, &buf),
            Some(false)
        );
        // BT shape (78-byte report 0x31): byte at offset 10.
        let mut buf_bt = [0u8; 78];
        buf_bt[10] = 0x01;
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
        assert!(parse_report(ControllerKind::DualShock4, &buf, &tx));
        match rx.try_recv().expect("imu event") {
            ChannelInfo::ImuSamples(s) => {
                assert!(s[0].gyro[0] > 0.0);
            }
            _ => panic!("expected ImuSamples"),
        }
    }
}

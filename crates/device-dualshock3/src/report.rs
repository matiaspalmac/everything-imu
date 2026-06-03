//! DualShock 3 input report `0x01` motion decode.
//!
//! Offsets are into the USB report **including** the leading `0x01` report-id
//! byte, matching how the Linux `hid-sony` driver indexes the motion words. Each
//! motion value is a 10-bit big-endian (MSB-first) `u16` centred near `512`.
//! ⚠ Scales are estimates pending hardware confirmation.

use device_traits::ImuSample;
use std::time::Instant;

const ACCEL_X: usize = 41;
const ACCEL_Y: usize = 43;
const ACCEL_Z: usize = 45;
const GYRO_Z: usize = 47;
/// Minimum length that still contains the gyro word (`47..=48`).
const MIN_LEN: usize = 49;

const G: f32 = 9.80665;
const ZERO: f32 = 512.0;
/// ≈ counts per g (KXPA4-class, estimate).
const ACCEL_LSB_PER_G: f32 = 113.0;
/// ≈ counts per rad/s for the single yaw axis (estimate, revision-dependent).
const GYRO_LSB_PER_RAD: f32 = 123.0;

/// Raw motion words straight off the wire (10-bit, already host-endian `u16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ds3Motion {
    pub accel: [u16; 3],
    /// Single-axis (yaw) gyroscope. The DS3 has no pitch/roll gyro.
    pub gyro_z: u16,
}

/// Decode the motion block from a DS3 input report. Returns `None` if this is
/// not a long-enough `0x01` report.
pub fn parse_input_report(buf: &[u8]) -> Option<Ds3Motion> {
    if buf.len() < MIN_LEN || buf[0] != 0x01 {
        return None;
    }
    // Motion words arrive MSB-first.
    let be = |o: usize| u16::from_be_bytes([buf[o], buf[o + 1]]);
    Some(Ds3Motion {
        accel: [be(ACCEL_X), be(ACCEL_Y), be(ACCEL_Z)],
        gyro_z: be(GYRO_Z),
    })
}

/// Map raw motion into an `ImuSample` (m/s² accel, rad/s gyro). Only the yaw
/// gyro axis is populated; X/Y are zero because the hardware has no sensor there.
///
/// ⚠ Axis convention is provisional (pass-through). Confirm gravity = +Z and the
/// yaw-gyro sign on a live pad before treating as canonical.
pub fn imu_from_motion(m: Ds3Motion, start: Instant, now: Instant) -> ImuSample {
    let accel_ms2 = |raw: u16| (raw as f32 - ZERO) / ACCEL_LSB_PER_G * G;
    let gyro_rad = (m.gyro_z as f32 - ZERO) / GYRO_LSB_PER_RAD;
    ImuSample {
        accel: [
            accel_ms2(m.accel[0]),
            accel_ms2(m.accel[1]),
            accel_ms2(m.accel[2]),
        ],
        gyro: [0.0, 0.0, gyro_rad],
        mag: None,
        timestamp_us: now.duration_since(start).as_micros() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_with_motion(ax: u16, ay: u16, az: u16, gz: u16) -> [u8; 49] {
        let mut r = [0u8; 49];
        r[0] = 0x01;
        r[ACCEL_X..ACCEL_X + 2].copy_from_slice(&ax.to_be_bytes());
        r[ACCEL_Y..ACCEL_Y + 2].copy_from_slice(&ay.to_be_bytes());
        r[ACCEL_Z..ACCEL_Z + 2].copy_from_slice(&az.to_be_bytes());
        r[GYRO_Z..GYRO_Z + 2].copy_from_slice(&gz.to_be_bytes());
        r
    }

    #[test]
    fn rejects_short_or_wrong_id() {
        assert!(parse_input_report(&[0x01u8; 10]).is_none());
        let mut r = report_with_motion(512, 512, 512, 512);
        r[0] = 0x02;
        assert!(parse_input_report(&r).is_none());
    }

    #[test]
    fn decodes_big_endian_motion_words() {
        let r = report_with_motion(512, 625, 400, 700);
        let m = parse_input_report(&r).expect("parse");
        assert_eq!(m.accel, [512, 625, 400]);
        assert_eq!(m.gyro_z, 700);
    }

    #[test]
    fn rest_maps_to_zero_accel_and_only_yaw_gyro() {
        // All axes at the 512 zero point, gyro at zero-rate.
        let r = report_with_motion(512, 512, 512, 512);
        let m = parse_input_report(&r).unwrap();
        let s = imu_from_motion(m, Instant::now(), Instant::now());
        assert!(s.accel.iter().all(|a| a.abs() < 0.01));
        assert_eq!(s.gyro[0], 0.0);
        assert_eq!(s.gyro[1], 0.0);
        assert!(s.gyro[2].abs() < 0.01);
    }

    #[test]
    fn one_g_offset_on_z() {
        // z one g above the zero point: 512 + 113 = 625.
        let r = report_with_motion(512, 512, 625, 512);
        let m = parse_input_report(&r).unwrap();
        let s = imu_from_motion(m, Instant::now(), Instant::now());
        assert!((s.accel[2] - G).abs() < 0.05, "got {}", s.accel[2]);
    }
}

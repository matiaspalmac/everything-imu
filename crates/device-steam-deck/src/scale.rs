//! Raw int16 → SI unit conversion for the Steam Deck BMI260.

use std::f32::consts::PI;

const STANDARD_GRAVITY: f32 = 9.806_65;
const GYRO_FULL_SCALE_DPS: f32 = 2000.0;
const ACCEL_FULL_SCALE_G: f32 = 2.0;

/// Convert a raw int16 gyro sample to rad/s.
///
/// Scale for the Steam Deck BMI260 (±2000 dps full scale):
///   `gyro_rad_s = (raw / 32768) * (2000 * π / 180)`
#[inline]
pub fn gyro_rad_s(raw: i16) -> f32 {
    (raw as f32 / 32768.0) * GYRO_FULL_SCALE_DPS * (PI / 180.0)
}

/// Convert a raw int16 accel sample to m/s².
///
///   `accel_m_s2 = (raw / 32768) * 2 * 9.80665`
#[inline]
pub fn accel_m_s2(raw: i16) -> f32 {
    (raw as f32 / 32768.0) * ACCEL_FULL_SCALE_G * STANDARD_GRAVITY
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn gyro_full_scale_positive() {
        // i16::MAX = 32767 ≈ 32768, so result ≈ 2000 dps in rad/s.
        let expected = 2000.0_f32 * (PI / 180.0);
        assert_relative_eq!(gyro_rad_s(i16::MAX), expected, max_relative = 1e-3);
    }

    #[test]
    fn gyro_full_scale_negative() {
        let expected = -2000.0_f32 * (PI / 180.0);
        assert_relative_eq!(gyro_rad_s(i16::MIN), expected, max_relative = 1e-3);
    }

    #[test]
    fn gyro_zero() {
        assert_eq!(gyro_rad_s(0), 0.0);
    }

    #[test]
    fn accel_full_scale_positive() {
        // 2 g in m/s².
        assert_relative_eq!(
            accel_m_s2(i16::MAX),
            2.0 * STANDARD_GRAVITY,
            max_relative = 1e-3
        );
    }

    #[test]
    fn accel_one_g_resting() {
        // BMI260 sitting flat on Z reports +16384 (half full-scale = +1 g).
        assert_relative_eq!(accel_m_s2(16384), STANDARD_GRAVITY, max_relative = 1e-3);
    }
}

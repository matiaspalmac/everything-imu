//! Raw int16 → SI unit conversion for the Steam Controller MPU-6500.
//!
//! Matches the Steam Deck scaling exactly (same firmware constants), so this
//! file is intentionally a parallel of `device-steam-deck::scale`.

use std::f32::consts::PI;

const STANDARD_GRAVITY: f32 = 9.806_65;
const GYRO_FULL_SCALE_DPS: f32 = 2000.0;
const ACCEL_FULL_SCALE_G: f32 = 2.0;

#[inline]
pub fn gyro_rad_s(raw: i16) -> f32 {
    (raw as f32 / 32768.0) * GYRO_FULL_SCALE_DPS * (PI / 180.0)
}

#[inline]
pub fn accel_m_s2(raw: i16) -> f32 {
    (raw as f32 / 32768.0) * ACCEL_FULL_SCALE_G * STANDARD_GRAVITY
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn one_g_resting_axis() {
        assert_relative_eq!(accel_m_s2(16384), STANDARD_GRAVITY, max_relative = 1e-3);
    }

    #[test]
    fn gyro_zero_is_zero() {
        assert_eq!(gyro_rad_s(0), 0.0);
    }

    #[test]
    fn gyro_max_under_full_scale() {
        // Conversion never exceeds the documented full-scale range.
        let max = gyro_rad_s(i16::MAX);
        assert!(max <= GYRO_FULL_SCALE_DPS * PI / 180.0 + 1e-3);
    }
}

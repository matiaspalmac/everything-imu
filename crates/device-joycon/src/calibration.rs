//! Calibration scale constants + raw-i16-to-SI conversion.
//!
//! Note: we multiply the accel result by GRAVITY to ship m/s² (architectural invariant #2).

pub const ACCEL_SCALE_RAW_TO_G: f32 = 16.0 / 65535.0;
pub const GRAVITY_M_PER_S2: f32 = 9.806_65;
pub const GYRO_SCALE_RAW_TO_DPS: f32 = 4588.0 / 65535.0;
pub const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

#[inline]
pub fn raw_minus_offset(raw: [i16; 3], offset: [i16; 3]) -> [f32; 3] {
    [
        (raw[0].wrapping_sub(offset[0])) as f32,
        (raw[1].wrapping_sub(offset[1])) as f32,
        (raw[2].wrapping_sub(offset[2])) as f32,
    ]
}

#[inline]
pub fn accel_to_m_s2(raw_minus_offset: [f32; 3]) -> [f32; 3] {
    let k = ACCEL_SCALE_RAW_TO_G * GRAVITY_M_PER_S2;
    [
        raw_minus_offset[0] * k,
        raw_minus_offset[1] * k,
        raw_minus_offset[2] * k,
    ]
}

#[inline]
pub fn gyro_to_rad_s(raw_minus_offset: [f32; 3]) -> [f32; 3] {
    let k = GYRO_SCALE_RAW_TO_DPS * DEG_TO_RAD;
    [
        raw_minus_offset[0] * k,
        raw_minus_offset[1] * k,
        raw_minus_offset[2] * k,
    ]
}

/// Convert a raw accel triple to m/s² using per-axis factory coefficients.
///
/// `coeff_g` is the dekuNukem `4.0/(sens-origin)` scale (g per LSB) from
/// [`crate::spi_cal::ImuCalibration`]; the result is multiplied by gravity to
/// satisfy the m/s² wire invariant. With nominal coefficients this is identical
/// to [`accel_to_m_s2`]; with real factory sensitivity it corrects the ~1 %
/// per-device gain spread the nominal path cannot.
#[inline]
pub fn accel_sample_m_s2(raw: [i16; 3], offset: [i16; 3], coeff_g: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0_f32; 3];
    for i in 0..3 {
        let v = raw[i].wrapping_sub(offset[i]) as f32;
        out[i] = v * coeff_g[i] * GRAVITY_M_PER_S2;
    }
    out
}

/// Convert a raw gyro triple to rad/s using per-axis factory coefficients.
///
/// `coeff_dps` is the dekuNukem `936.0/(sens-offset)` scale (dps per LSB).
#[inline]
pub fn gyro_sample_rad_s(raw: [i16; 3], offset: [i16; 3], coeff_dps: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0_f32; 3];
    for i in 0..3 {
        let v = raw[i].wrapping_sub(offset[i]) as f32;
        out[i] = v * coeff_dps[i] * DEG_TO_RAD;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accel_face_up_one_g() {
        let r = accel_to_m_s2(raw_minus_offset([4096, 0, 0], [0, 0, 0]));
        assert!((r[0] - 9.806_65).abs() < 0.05, "got {}", r[0]);
    }

    #[test]
    fn gyro_zero_offset_zero_output() {
        let r = gyro_to_rad_s(raw_minus_offset([0, 0, 0], [0, 0, 0]));
        assert_eq!(r, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn gyro_one_lsb_at_constant_scale() {
        let r = gyro_to_rad_s(raw_minus_offset([1, 0, 0], [0, 0, 0]));
        let expected = (4588.0_f32 / 65535.0) * (std::f32::consts::PI / 180.0);
        assert!((r[0] - expected).abs() < 1e-7);
    }

    #[test]
    fn raw_minus_offset_subtracts() {
        let r = raw_minus_offset([100, 200, -50], [50, 100, -25]);
        assert_eq!(r, [50.0, 100.0, -25.0]);
    }

    #[test]
    fn accel_sample_with_nominal_coeff_matches_legacy_path() {
        let coeff = [ACCEL_SCALE_RAW_TO_G; 3];
        let new = accel_sample_m_s2([4096, 0, 0], [0, 0, 0], coeff);
        let legacy = accel_to_m_s2(raw_minus_offset([4096, 0, 0], [0, 0, 0]));
        assert!((new[0] - legacy[0]).abs() < 1e-4);
        assert!((new[0] - 9.806_65).abs() < 0.05);
    }

    #[test]
    fn gyro_sample_applies_offset_and_per_axis_coeff() {
        // Offset subtracted, then per-axis dps coeff, then deg→rad.
        let coeff = [0.07_f32; 3];
        let r = gyro_sample_rad_s([110, 0, 0], [10, 0, 0], coeff);
        let expected = 100.0 * 0.07 * DEG_TO_RAD;
        assert!((r[0] - expected).abs() < 1e-6);
    }
}

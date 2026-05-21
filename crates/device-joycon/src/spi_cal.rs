//! SPI flash calibration block parser.

const ACCEL_OFFSET_PLAUSIBLE_ABS: i16 = 2000;
const GYRO_OFFSET_PLAUSIBLE_ABS: i16 = 1000;

pub const USER_OVERRIDE_MAGIC: u16 = 0xA1B2;

// Nominal IMU coefficients — dekuNukem `imu_sensor_notes.md`:
//   accel: coeff = 4.0 / (cal_acc_sens - cal_acc_origin),  default sens 0x4000 = 16384
//   gyro : coeff = 936.0 / (cal_gyro_sens - cal_gyro_offset), default sens 0x343B = 13371
// These defaults are what a controller reports when no per-device factory
// sensitivity is programmed, so they are the correct fallback (not the chip's
// ±8 G / ±2000 dps spec values, which are ~13-19 % off).
const ACCEL_COEFF_NUMERATOR: f32 = 4.0;
const GYRO_COEFF_NUMERATOR: f32 = 936.0;
const NOMINAL_ACCEL_COEFF_G: f32 = ACCEL_COEFF_NUMERATOR / 16384.0;
const NOMINAL_GYRO_COEFF_DPS: f32 = GYRO_COEFF_NUMERATOR / 13371.0;

#[derive(Debug, Clone, Copy)]
pub struct ImuCalibration {
    pub accel_offset: [i16; 3],
    pub accel_sensitivity: Option<[i16; 3]>,
    pub gyro_offset: [i16; 3],
    pub gyro_sensitivity: Option<[i16; 3]>,
    /// Per-axis accel scale, g per LSB. Derived from factory sensitivity when
    /// present, else [`NOMINAL_ACCEL_COEFF_G`].
    pub accel_coeff_g: [f32; 3],
    /// Per-axis gyro scale, dps per LSB. Derived from factory sensitivity when
    /// present, else [`NOMINAL_GYRO_COEFF_DPS`].
    pub gyro_coeff_dps: [f32; 3],
    pub plausibility_warning: bool,
}

impl ImuCalibration {
    pub fn zero() -> Self {
        Self {
            accel_offset: [0; 3],
            accel_sensitivity: None,
            gyro_offset: [0; 3],
            gyro_sensitivity: None,
            accel_coeff_g: [NOMINAL_ACCEL_COEFF_G; 3],
            gyro_coeff_dps: [NOMINAL_GYRO_COEFF_DPS; 3],
            plausibility_warning: false,
        }
    }
}

/// Compute the per-axis dekuNukem coefficient `numerator / (sensitivity - offset)`.
///
/// Falls back to `nominal` for any axis where the factory sensitivity is absent,
/// the denominator is degenerate, or the result lands outside a 0.5×–2.0× band
/// around nominal (a corrupt SPI read that still passed the offset plausibility
/// gate would otherwise inject a wildly wrong scale).
fn axis_coeffs(
    sensitivity: Option<[i16; 3]>,
    offset: [i16; 3],
    numerator: f32,
    nominal: f32,
) -> [f32; 3] {
    let Some(sens) = sensitivity else {
        return [nominal; 3];
    };
    let mut out = [nominal; 3];
    for i in 0..3 {
        let denom = sens[i] as f32 - offset[i] as f32;
        if denom.abs() <= 1.0 {
            continue;
        }
        let coeff = numerator / denom;
        if coeff.is_finite() && coeff > nominal * 0.5 && coeff < nominal * 2.0 {
            out[i] = coeff;
        }
    }
    out
}

#[derive(thiserror::Error, Debug)]
pub enum SpiError {
    #[error("expected 24-byte cal block, got {0}")]
    WrongLength(usize),
}

pub fn parse_factory_block(block: &[u8]) -> Result<ImuCalibration, SpiError> {
    if block.len() < 24 {
        return Err(SpiError::WrongLength(block.len()));
    }
    let read_i16_3 = |off: usize| -> [i16; 3] {
        [
            i16::from_le_bytes([block[off], block[off + 1]]),
            i16::from_le_bytes([block[off + 2], block[off + 3]]),
            i16::from_le_bytes([block[off + 4], block[off + 5]]),
        ]
    };

    let accel_offset = read_i16_3(0);
    let accel_sens_raw = read_i16_3(6);
    let gyro_offset = read_i16_3(12);
    let gyro_sens_raw = read_i16_3(18);

    let accel_sensitivity = if is_zeros_or_ff(&block[6..12]) {
        None
    } else {
        Some(accel_sens_raw)
    };
    let gyro_sensitivity = if is_zeros_or_ff(&block[18..24]) {
        None
    } else {
        Some(gyro_sens_raw)
    };

    let plausibility_warning = !plausible_offset(accel_offset, ACCEL_OFFSET_PLAUSIBLE_ABS)
        || !plausible_offset(gyro_offset, GYRO_OFFSET_PLAUSIBLE_ABS);

    let (accel_offset, gyro_offset) = if plausibility_warning {
        ([0; 3], [0; 3])
    } else {
        (accel_offset, gyro_offset)
    };

    // An implausible block is not trustworthy for sensitivity either — fall
    // back to nominal coefficients across the board.
    let (accel_coeff_g, gyro_coeff_dps) = if plausibility_warning {
        ([NOMINAL_ACCEL_COEFF_G; 3], [NOMINAL_GYRO_COEFF_DPS; 3])
    } else {
        (
            axis_coeffs(
                accel_sensitivity,
                accel_offset,
                ACCEL_COEFF_NUMERATOR,
                NOMINAL_ACCEL_COEFF_G,
            ),
            axis_coeffs(
                gyro_sensitivity,
                gyro_offset,
                GYRO_COEFF_NUMERATOR,
                NOMINAL_GYRO_COEFF_DPS,
            ),
        )
    };

    Ok(ImuCalibration {
        accel_offset,
        accel_sensitivity,
        gyro_offset,
        gyro_sensitivity,
        accel_coeff_g,
        gyro_coeff_dps,
        plausibility_warning,
    })
}

pub fn user_override_magic_present(magic_bytes: &[u8]) -> bool {
    if magic_bytes.len() < 2 {
        return false;
    }
    let word = u16::from_le_bytes([magic_bytes[0], magic_bytes[1]]);
    word == USER_OVERRIDE_MAGIC
}

fn is_zeros_or_ff(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == 0x00) || bytes.iter().all(|&b| b == 0xFF)
}

fn plausible_offset(values: [i16; 3], abs_max: i16) -> bool {
    values.iter().all(|&v| v.abs() <= abs_max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_normal() -> Vec<u8> {
        let mut b = vec![0u8; 24];
        b[0..2].copy_from_slice(&10i16.to_le_bytes());
        b[2..4].copy_from_slice(&20i16.to_le_bytes());
        b[4..6].copy_from_slice(&30i16.to_le_bytes());
        b[6..8].copy_from_slice(&16384i16.to_le_bytes());
        b[8..10].copy_from_slice(&16384i16.to_le_bytes());
        b[10..12].copy_from_slice(&16384i16.to_le_bytes());
        b[12..14].copy_from_slice(&(-5i16).to_le_bytes());
        b[14..16].copy_from_slice(&7i16.to_le_bytes());
        b[16..18].copy_from_slice(&(-3i16).to_le_bytes());
        b[18..20].copy_from_slice(&13371i16.to_le_bytes());
        b[20..22].copy_from_slice(&13371i16.to_le_bytes());
        b[22..24].copy_from_slice(&13371i16.to_le_bytes());
        b
    }

    #[test]
    fn parses_factory_offset_and_sensitivity() {
        let cal = parse_factory_block(&fixture_normal()).unwrap();
        assert_eq!(cal.accel_offset, [10, 20, 30]);
        assert_eq!(cal.gyro_offset, [-5, 7, -3]);
        assert_eq!(cal.accel_sensitivity, Some([16384, 16384, 16384]));
        assert_eq!(cal.gyro_sensitivity, Some([13371, 13371, 13371]));
        assert!(!cal.plausibility_warning);
    }

    #[test]
    fn clone_unit_zero_flash_falls_back() {
        let cal = parse_factory_block(&[0u8; 24]).unwrap();
        assert_eq!(cal.accel_offset, [0; 3]);
        assert_eq!(cal.accel_sensitivity, None);
        assert_eq!(cal.gyro_sensitivity, None);
        assert!(!cal.plausibility_warning);
    }

    #[test]
    fn ff_flash_treated_as_missing_sensitivity() {
        let cal = parse_factory_block(&[0xFFu8; 24]).unwrap();
        assert_eq!(cal.accel_sensitivity, None);
        assert_eq!(cal.gyro_sensitivity, None);
        assert!(!cal.plausibility_warning);
    }

    #[test]
    fn implausible_offset_warns_and_zeros() {
        let mut b = vec![0u8; 24];
        b[0..2].copy_from_slice(&5000i16.to_le_bytes());
        let cal = parse_factory_block(&b).unwrap();
        assert!(cal.plausibility_warning);
        assert_eq!(cal.accel_offset, [0; 3]);
    }

    #[test]
    fn coeffs_derived_from_factory_sensitivity() {
        let cal = parse_factory_block(&fixture_normal()).unwrap();
        // accel: 4.0 / (16384 - 10) on axis 0.
        assert!((cal.accel_coeff_g[0] - 4.0 / (16384.0 - 10.0)).abs() < 1e-9);
        // gyro: 936.0 / (13371 - (-5)) on axis 0.
        assert!((cal.gyro_coeff_dps[0] - 936.0 / (13371.0 + 5.0)).abs() < 1e-6);
    }

    #[test]
    fn coeffs_fall_back_to_nominal_without_sensitivity() {
        // All-zero flash → sensitivity None → nominal coefficients.
        let cal = parse_factory_block(&[0u8; 24]).unwrap();
        assert!((cal.accel_coeff_g[0] - NOMINAL_ACCEL_COEFF_G).abs() < 1e-9);
        assert!((cal.gyro_coeff_dps[0] - NOMINAL_GYRO_COEFF_DPS).abs() < 1e-9);
    }

    #[test]
    fn coeffs_reject_out_of_band_sensitivity() {
        // Sensitivity far too small → coeff would be >2× nominal → rejected,
        // axis keeps the nominal value.
        let mut b = vec![0u8; 24];
        b[6..8].copy_from_slice(&100i16.to_le_bytes());
        b[8..10].copy_from_slice(&100i16.to_le_bytes());
        b[10..12].copy_from_slice(&100i16.to_le_bytes());
        let cal = parse_factory_block(&b).unwrap();
        assert!((cal.accel_coeff_g[0] - NOMINAL_ACCEL_COEFF_G).abs() < 1e-9);
    }

    #[test]
    fn implausible_block_uses_nominal_coeffs() {
        let mut b = vec![0u8; 24];
        b[0..2].copy_from_slice(&5000i16.to_le_bytes()); // implausible accel offset
        b[6..8].copy_from_slice(&16384i16.to_le_bytes());
        let cal = parse_factory_block(&b).unwrap();
        assert!(cal.plausibility_warning);
        assert!((cal.accel_coeff_g[0] - NOMINAL_ACCEL_COEFF_G).abs() < 1e-9);
    }

    #[test]
    fn user_override_magic_recognized() {
        assert!(user_override_magic_present(&[0xB2, 0xA1]));
    }

    #[test]
    fn user_override_missing_magic() {
        assert!(!user_override_magic_present(&[0xFF, 0xFF]));
        assert!(!user_override_magic_present(&[0x00, 0x00]));
    }
}

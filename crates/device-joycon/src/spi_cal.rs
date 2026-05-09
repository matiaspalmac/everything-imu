//! SPI flash calibration block parser.

const ACCEL_OFFSET_PLAUSIBLE_ABS: i16 = 2000;
const GYRO_OFFSET_PLAUSIBLE_ABS: i16 = 1000;

pub const USER_OVERRIDE_MAGIC: u16 = 0xA1B2;

#[derive(Debug, Clone, Copy)]
pub struct ImuCalibration {
    pub accel_offset: [i16; 3],
    pub accel_sensitivity: Option<[i16; 3]>,
    pub gyro_offset: [i16; 3],
    pub gyro_sensitivity: Option<[i16; 3]>,
    pub plausibility_warning: bool,
}

impl ImuCalibration {
    pub fn zero() -> Self {
        Self {
            accel_offset: [0; 3],
            accel_sensitivity: None,
            gyro_offset: [0; 3],
            gyro_sensitivity: None,
            plausibility_warning: false,
        }
    }
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

    Ok(ImuCalibration {
        accel_offset,
        accel_sensitivity,
        gyro_offset,
        gyro_sensitivity,
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
    fn user_override_magic_recognized() {
        assert!(user_override_magic_present(&[0xB2, 0xA1]));
    }

    #[test]
    fn user_override_missing_magic() {
        assert!(!user_override_magic_present(&[0xFF, 0xFF]));
        assert!(!user_override_magic_present(&[0x00, 0x00]));
    }
}

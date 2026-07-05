//! Parser for the Steam Deck gamepad HID state input report.
//!
//! Layout reverse-engineered from the device's own feature/state reports:
//!
//! ```text
//! offset  size  field
//! ------  ----  ---------------------------------------------------
//!   0       1   report_id (0x01 = state)
//!   1       1   reserved
//!   2       2   report_version (le u16)
//!   4       4   seq (le u32) — monotonically increasing
//!   8       8   ulButtonsL (le u64, only low 32 bits used)
//!  16       4   ulButtonsH (le u32, high button bits)
//!  20       2   left_pad_x (i16)
//!  22       2   left_pad_y (i16)
//!  24       2   right_pad_x (i16)
//!  26       2   right_pad_y (i16)
//!  28       2   accel_x (i16) ±2 g
//!  30       2   accel_y (i16)
//!  32       2   accel_z (i16)
//!  34       2   gyro_x  (i16) ±2000 dps
//!  36       2   gyro_y  (i16)
//!  38       2   gyro_z  (i16)
//!  40       2   quat_w  (i16) — onboard fusion (optional)
//!  42       2   quat_x  (i16)
//!  44       2   quat_y  (i16)
//!  46       2   quat_z  (i16)
//!  48       2   trigger_l_raw (u16)
//!  50       2   trigger_r_raw (u16)
//!  52       2   left_stick_x (i16)
//!  54       2   left_stick_y (i16)
//!  56       2   right_stick_x (i16)
//!  58       2   right_stick_y (i16)
//! ```
//!
//! Total minimum useful length: 60 bytes (full reports are 64).

pub const REPORT_ID_STATE: u8 = 0x01;
pub const MIN_REPORT_LEN: usize = 60;

#[derive(Debug, Clone, Copy)]
pub struct DeckInputReport {
    pub seq: u32,
    pub accel_raw: [i16; 3],
    pub gyro_raw: [i16; 3],
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ReportError {
    #[error("report too short: {0} < {MIN_REPORT_LEN}")]
    TooShort(usize),
    #[error("unexpected report id 0x{0:02X}")]
    BadReportId(u8),
}

pub fn parse(buf: &[u8]) -> Result<DeckInputReport, ReportError> {
    if buf.len() < MIN_REPORT_LEN {
        return Err(ReportError::TooShort(buf.len()));
    }
    if buf[0] != REPORT_ID_STATE {
        return Err(ReportError::BadReportId(buf[0]));
    }
    let seq = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let rd_i16 = |off: usize| i16::from_le_bytes([buf[off], buf[off + 1]]);
    Ok(DeckInputReport {
        seq,
        accel_raw: [rd_i16(28), rd_i16(30), rd_i16(32)],
        gyro_raw: [rd_i16(34), rd_i16(36), rd_i16(38)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fixture(seq: u32, accel: [i16; 3], gyro: [i16; 3]) -> Vec<u8> {
        let mut buf = vec![0u8; 64];
        buf[0] = REPORT_ID_STATE;
        buf[4..8].copy_from_slice(&seq.to_le_bytes());
        for (i, v) in accel.iter().enumerate() {
            buf[28 + i * 2..30 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in gyro.iter().enumerate() {
            buf[34 + i * 2..36 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        buf
    }

    #[test]
    fn round_trip_known_fixture() {
        let buf = make_fixture(0xABCD_1234, [100, -200, 16384], [-500, 600, -700]);
        let r = parse(&buf).unwrap();
        assert_eq!(r.seq, 0xABCD_1234);
        assert_eq!(r.accel_raw, [100, -200, 16384]);
        assert_eq!(r.gyro_raw, [-500, 600, -700]);
    }

    #[test]
    fn rejects_short_buffer() {
        assert_eq!(parse(&[0u8; 10]).unwrap_err(), ReportError::TooShort(10));
    }

    #[test]
    fn rejects_wrong_report_id() {
        let mut buf = vec![0u8; 64];
        buf[0] = 0x09;
        assert_eq!(parse(&buf).unwrap_err(), ReportError::BadReportId(0x09));
    }

    #[test]
    fn parses_minimum_length() {
        let mut buf = vec![0u8; MIN_REPORT_LEN];
        buf[0] = REPORT_ID_STATE;
        buf[28..30].copy_from_slice(&16_384_i16.to_le_bytes());
        let r = parse(&buf).unwrap();
        assert_eq!(r.accel_raw[0], 16_384);
    }
}

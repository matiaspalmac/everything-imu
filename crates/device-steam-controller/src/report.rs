//! Steam Controller input-report parser.
//!
//! Layout reverse-engineered from SDL `SDL_hidapi_steam.c` (Valve
//! `controller_structs.h: SteamControllerStateInternal_t`). The 64-byte
//! state report begins with a 4-byte header and packs IMU data starting at
//! offset 0x14 (20) in the order accel, gyro, quaternion (each 3×i16 / 4×i16).
//!
//! Different firmware reports use different leading wrappers (over USB the
//! state arrives wrapped in a `0x01` ID byte; over BLE it is reassembled from
//! 18-byte segments before reaching this parser). The parser takes the raw
//! 64-byte state body — wrapper stripping is the transport layer's job.

pub const STATE_BODY_LEN: usize = 64;
pub const IMU_OFFSET_ACCEL: usize = 20;
pub const IMU_OFFSET_GYRO: usize = 26;
pub const IMU_OFFSET_QUAT: usize = 32;

#[derive(Debug, Clone, Copy)]
pub struct SteamControllerState {
    pub accel_raw: [i16; 3],
    pub gyro_raw: [i16; 3],
    pub quat_raw: [i16; 4],
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ReportError {
    #[error("state body too short: {0} < {STATE_BODY_LEN}")]
    TooShort(usize),
}

pub fn parse_state(body: &[u8]) -> Result<SteamControllerState, ReportError> {
    if body.len() < STATE_BODY_LEN {
        return Err(ReportError::TooShort(body.len()));
    }
    let rd_i16 = |off: usize| i16::from_le_bytes([body[off], body[off + 1]]);
    Ok(SteamControllerState {
        accel_raw: [
            rd_i16(IMU_OFFSET_ACCEL),
            rd_i16(IMU_OFFSET_ACCEL + 2),
            rd_i16(IMU_OFFSET_ACCEL + 4),
        ],
        gyro_raw: [
            rd_i16(IMU_OFFSET_GYRO),
            rd_i16(IMU_OFFSET_GYRO + 2),
            rd_i16(IMU_OFFSET_GYRO + 4),
        ],
        quat_raw: [
            rd_i16(IMU_OFFSET_QUAT),
            rd_i16(IMU_OFFSET_QUAT + 2),
            rd_i16(IMU_OFFSET_QUAT + 4),
            rd_i16(IMU_OFFSET_QUAT + 6),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_body(accel: [i16; 3], gyro: [i16; 3], quat: [i16; 4]) -> Vec<u8> {
        let mut buf = vec![0u8; STATE_BODY_LEN];
        for (i, v) in accel.iter().enumerate() {
            buf[IMU_OFFSET_ACCEL + i * 2..IMU_OFFSET_ACCEL + i * 2 + 2]
                .copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in gyro.iter().enumerate() {
            buf[IMU_OFFSET_GYRO + i * 2..IMU_OFFSET_GYRO + i * 2 + 2]
                .copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in quat.iter().enumerate() {
            buf[IMU_OFFSET_QUAT + i * 2..IMU_OFFSET_QUAT + i * 2 + 2]
                .copy_from_slice(&v.to_le_bytes());
        }
        buf
    }

    #[test]
    fn round_trip_known_values() {
        let body = make_body([1, -2, 16384], [10, -20, 30], [32767, 0, -32768, 100]);
        let s = parse_state(&body).unwrap();
        assert_eq!(s.accel_raw, [1, -2, 16384]);
        assert_eq!(s.gyro_raw, [10, -20, 30]);
        assert_eq!(s.quat_raw, [32767, 0, -32768, 100]);
    }

    #[test]
    fn rejects_short_body() {
        assert_eq!(
            parse_state(&[0u8; 30]).unwrap_err(),
            ReportError::TooShort(30)
        );
    }

    #[test]
    fn empty_body_is_zero_imu() {
        let body = vec![0u8; STATE_BODY_LEN];
        let s = parse_state(&body).unwrap();
        assert_eq!(s.accel_raw, [0; 3]);
        assert_eq!(s.gyro_raw, [0; 3]);
        assert_eq!(s.quat_raw, [0; 4]);
    }
}

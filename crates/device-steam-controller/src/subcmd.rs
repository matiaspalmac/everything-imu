//! Feature-report builders for the Steam Controller HID control plane.
//!
//! All Steam Controller commands ride a 65-byte HID feature report (1 byte
//! report ID = 0, 64 bytes payload). The payload is a TLV sequence:
//!
//! ```text
//! payload[0]      = command_id
//! payload[1]      = command_length (excludes the command_id + length bytes)
//! payload[2..N]   = command data
//! ```
//!
//! Multiple TLV commands MAY chain inside one report; we only emit single
//! commands which is sufficient for the IMU enable path.

pub const REPORT_LEN: usize = 65;

// Command IDs from Valve's `controller_constants.h`.
pub const ID_GET_ATTRIBUTES_VALUES: u8 = 0x83;
pub const ID_CLEAR_DIGITAL_MAPPINGS: u8 = 0x81;
pub const ID_LOAD_DEFAULT_SETTINGS: u8 = 0x8E;
pub const ID_SET_SETTINGS_VALUES: u8 = 0x87;

// Settings keys (TLV inside ID_SET_SETTINGS_VALUES payload).
pub const SETTING_IMU_MODE: u8 = 0x30;

// Bitmask values for SETTING_IMU_MODE.
pub const SETTING_GYRO_MODE_OFF: u16 = 0x0000;
pub const SETTING_GYRO_MODE_SEND_RAW_ACCEL: u16 = 0x0001;
pub const SETTING_GYRO_MODE_SEND_RAW_GYRO: u16 = 0x0002;
pub const SETTING_GYRO_MODE_SEND_RAW_QUATERNION: u16 = 0x0004;

pub fn build_simple(command_id: u8) -> [u8; REPORT_LEN] {
    let mut out = [0u8; REPORT_LEN];
    out[0] = 0; // HID feature reports use ID 0 on the Steam Controller.
    out[1] = command_id;
    out[2] = 0; // length = 0
    out
}

pub fn build_set_setting_u16(key: u8, value: u16) -> [u8; REPORT_LEN] {
    let mut out = [0u8; REPORT_LEN];
    out[0] = 0;
    out[1] = ID_SET_SETTINGS_VALUES;
    out[2] = 3; // length = 1 key + 2 value bytes
    out[3] = key;
    out[4] = (value & 0xFF) as u8;
    out[5] = (value >> 8) as u8;
    out
}

pub fn enable_imu_raw() -> [u8; REPORT_LEN] {
    let mode = SETTING_GYRO_MODE_SEND_RAW_ACCEL | SETTING_GYRO_MODE_SEND_RAW_GYRO;
    build_set_setting_u16(SETTING_IMU_MODE, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_command_layout() {
        let buf = build_simple(ID_CLEAR_DIGITAL_MAPPINGS);
        assert_eq!(buf.len(), REPORT_LEN);
        assert_eq!(buf[0], 0);
        assert_eq!(buf[1], ID_CLEAR_DIGITAL_MAPPINGS);
        assert_eq!(buf[2], 0);
        assert!(buf[3..].iter().all(|&b| b == 0));
    }

    #[test]
    fn set_setting_layout() {
        let buf = build_set_setting_u16(SETTING_IMU_MODE, 0xCAFE);
        assert_eq!(buf[1], ID_SET_SETTINGS_VALUES);
        assert_eq!(buf[2], 3);
        assert_eq!(buf[3], SETTING_IMU_MODE);
        assert_eq!(buf[4], 0xFE);
        assert_eq!(buf[5], 0xCA);
    }

    #[test]
    fn enable_imu_sets_accel_and_gyro_bits() {
        let buf = enable_imu_raw();
        let value = u16::from_le_bytes([buf[4], buf[5]]);
        assert_eq!(
            value,
            SETTING_GYRO_MODE_SEND_RAW_ACCEL | SETTING_GYRO_MODE_SEND_RAW_GYRO
        );
        // Sanity: the quaternion bit must NOT be set in the basic enable
        // (caller can request it explicitly via build_set_setting_u16).
        assert_eq!(value & SETTING_GYRO_MODE_SEND_RAW_QUATERNION, 0);
    }
}

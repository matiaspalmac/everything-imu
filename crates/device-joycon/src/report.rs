//! Input report 0x30 parser (49-byte full-mode layout).

use device_traits::BatteryState;

#[derive(Debug, Clone, Copy)]
pub struct ImuRawSample {
    pub accel: [i16; 3],
    pub gyro: [i16; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct ButtonsRaw {
    pub low: u8,
    pub high: u8,
    pub shared: u8,
}

#[derive(Debug, Clone)]
pub struct InputReport0x30 {
    pub timer: u8,
    pub battery: BatteryState,
    pub buttons: ButtonsRaw,
    pub imu_samples: [ImuRawSample; 3],
}

#[derive(thiserror::Error, Debug)]
pub enum ReportError {
    #[error("expected 49-byte report, got {0}")]
    WrongLength(usize),
    #[error("expected report id 0x30, got {0:#04x}")]
    WrongId(u8),
}

pub fn parse_0x30(buf: &[u8]) -> Result<InputReport0x30, ReportError> {
    if buf.len() < 49 {
        return Err(ReportError::WrongLength(buf.len()));
    }
    if buf[0] != 0x30 {
        return Err(ReportError::WrongId(buf[0]));
    }

    let timer = buf[1];
    let battery = decode_battery(buf[2]);
    let buttons = ButtonsRaw {
        low: buf[3],
        high: buf[4],
        shared: buf[5],
    };

    let imu_samples = [
        parse_imu_sample(&buf[13..25]),
        parse_imu_sample(&buf[25..37]),
        parse_imu_sample(&buf[37..49]),
    ];

    Ok(InputReport0x30 {
        timer,
        battery,
        buttons,
        imu_samples,
    })
}

fn parse_imu_sample(slice: &[u8]) -> ImuRawSample {
    debug_assert!(slice.len() == 12);
    let read_i16_le = |off: usize| i16::from_le_bytes([slice[off], slice[off + 1]]);
    ImuRawSample {
        accel: [read_i16_le(0), read_i16_le(2), read_i16_le(4)],
        gyro: [read_i16_le(6), read_i16_le(8), read_i16_le(10)],
    }
}

/// Subcommand reply input report 0x21.
///
/// Layout per dekuNukem `bluetooth_hid_subcommands_notes.md`:
/// - [0]      report id (0x21)
/// - [1]      timer
/// - [2]      battery byte
/// - [3..6]   buttons
/// - [6..13]  stick + vibrator
/// - [13]     ack byte (0x90 = OK for SPI read)
/// - [14]     subcommand replied
/// - [15..]   subcommand payload
///
/// For SPI read (subcmd 0x10) the payload is:
/// - [15..19] requested addr (u32 LE)
/// - [19]     requested len
/// - [20..]   data (`len` bytes)
#[derive(Debug, Clone)]
pub struct SpiReadReply {
    pub addr: u32,
    pub data: Vec<u8>,
}

pub fn parse_0x21_spi_reply(buf: &[u8]) -> Option<SpiReadReply> {
    if buf.len() < 20 {
        return None;
    }
    if buf[0] != 0x21 {
        return None;
    }
    if buf[14] != 0x10 {
        return None;
    }
    let addr = u32::from_le_bytes([buf[15], buf[16], buf[17], buf[18]]);
    let len = buf[19] as usize;
    if buf.len() < 20 + len {
        return None;
    }
    Some(SpiReadReply {
        addr,
        data: buf[20..20 + len].to_vec(),
    })
}

pub fn decode_battery(byte: u8) -> BatteryState {
    let level = (byte >> 5) & 0x07;
    let charging = (byte & 0x10) != 0;
    let fraction = match level {
        4 => 1.0,
        3 => 0.75,
        2 => 0.50,
        1 => 0.25,
        _ => 0.05,
    };
    BatteryState { fraction, charging }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(timer: u8, battery_byte: u8) -> Vec<u8> {
        let mut buf = vec![0u8; 49];
        buf[0] = 0x30;
        buf[1] = timer;
        buf[2] = battery_byte;
        for (off, val) in [(13, 1i16), (15, 2), (17, 3), (19, 4), (21, 5), (23, 6)] {
            let bytes = val.to_le_bytes();
            buf[off] = bytes[0];
            buf[off + 1] = bytes[1];
        }
        buf
    }

    #[test]
    fn parses_timer_and_first_sample() {
        let buf = fixture(0xAA, 0b1000_0000);
        let r = parse_0x30(&buf).unwrap();
        assert_eq!(r.timer, 0xAA);
        assert_eq!(r.imu_samples[0].accel, [1, 2, 3]);
        assert_eq!(r.imu_samples[0].gyro, [4, 5, 6]);
    }

    #[test]
    fn battery_full_charging() {
        let s = decode_battery(0b1001_0000);
        assert_eq!(s.fraction, 1.0);
        assert!(s.charging);
    }

    #[test]
    fn battery_critical_not_charging() {
        let s = decode_battery(0b0000_0000);
        assert_eq!(s.fraction, 0.05);
        assert!(!s.charging);
    }

    #[test]
    fn rejects_short_buffer() {
        let buf = vec![0u8; 10];
        assert!(matches!(parse_0x30(&buf), Err(ReportError::WrongLength(_))));
    }

    #[test]
    fn rejects_wrong_id() {
        let mut buf = vec![0u8; 49];
        buf[0] = 0x21;
        assert!(matches!(parse_0x30(&buf), Err(ReportError::WrongId(_))));
    }

    fn fixture_0x21_spi(addr: u32, data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 20 + data.len()];
        buf[0] = 0x21;
        buf[13] = 0x90; // ack
        buf[14] = 0x10; // SPI read subcmd
        let a = addr.to_le_bytes();
        buf[15] = a[0];
        buf[16] = a[1];
        buf[17] = a[2];
        buf[18] = a[3];
        buf[19] = data.len() as u8;
        buf[20..20 + data.len()].copy_from_slice(data);
        buf
    }

    #[test]
    fn parse_0x21_factory_block() {
        let data = vec![0xAB; 24];
        let buf = fixture_0x21_spi(0x6020, &data);
        let r = parse_0x21_spi_reply(&buf).unwrap();
        assert_eq!(r.addr, 0x6020);
        assert_eq!(r.data, data);
    }

    #[test]
    fn parse_0x21_rejects_short() {
        assert!(parse_0x21_spi_reply(&[0x21]).is_none());
    }

    #[test]
    fn parse_0x21_rejects_wrong_subcmd() {
        let mut buf = vec![0u8; 30];
        buf[0] = 0x21;
        buf[14] = 0x40; // not SPI read
        assert!(parse_0x21_spi_reply(&buf).is_none());
    }
}

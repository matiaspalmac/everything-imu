//! Output-report and subcommand builders.

const NEUTRAL_RUMBLE_FRAME: [u8; 4] = [0x00, 0x01, 0x40, 0x40];

pub fn build_report_0x01(packet_counter: u8, subcmd_id: u8, subcmd_data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(11 + subcmd_data.len());
    buf.push(0x01);
    buf.push(packet_counter & 0x0F);
    buf.extend_from_slice(&NEUTRAL_RUMBLE_FRAME);
    buf.extend_from_slice(&NEUTRAL_RUMBLE_FRAME);
    buf.push(subcmd_id);
    buf.extend_from_slice(subcmd_data);
    buf
}

pub fn build_report_0x80(opcode: u8) -> Vec<u8> {
    vec![0x80, opcode]
}

pub fn enable_imu(packet_counter: u8) -> Vec<u8> {
    build_report_0x01(packet_counter, 0x40, &[0x01])
}

pub fn enable_rumble(packet_counter: u8) -> Vec<u8> {
    build_report_0x01(packet_counter, 0x48, &[0x01])
}

pub fn set_player_leds(packet_counter: u8, mask: u8) -> Vec<u8> {
    build_report_0x01(packet_counter, 0x30, &[mask])
}

pub fn set_input_report_mode(packet_counter: u8, mode: u8) -> Vec<u8> {
    build_report_0x01(packet_counter, 0x03, &[mode])
}

pub fn spi_read(packet_counter: u8, addr: u32, len: u8) -> Vec<u8> {
    let bytes = addr.to_le_bytes();
    build_report_0x01(
        packet_counter,
        0x10,
        &[bytes[0], bytes[1], bytes[2], bytes[3], len],
    )
}

pub fn device_info(packet_counter: u8) -> Vec<u8> {
    build_report_0x01(packet_counter, 0x02, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_0x01_layout() {
        let p = build_report_0x01(0xA5, 0x40, &[0x01]);
        assert_eq!(p[0], 0x01);
        assert_eq!(p[1], 0x05);
        assert_eq!(&p[2..6], &NEUTRAL_RUMBLE_FRAME);
        assert_eq!(&p[6..10], &NEUTRAL_RUMBLE_FRAME);
        assert_eq!(p[10], 0x40);
        assert_eq!(p[11], 0x01);
    }

    #[test]
    fn enable_imu_payload() {
        let p = enable_imu(0);
        assert_eq!(p[10], 0x40);
        assert_eq!(p[11], 0x01);
    }

    #[test]
    fn spi_read_addr_le() {
        let p = spi_read(0, 0x6020, 24);
        assert_eq!(&p[11..15], &[0x20, 0x60, 0x00, 0x00]);
        assert_eq!(p[15], 24);
    }

    #[test]
    fn report_0x80_minimal() {
        let p = build_report_0x80(0x02);
        assert_eq!(p, vec![0x80, 0x02]);
    }

    #[test]
    fn set_player_leds_mask_passthrough() {
        let p = set_player_leds(0, 0b0000_0001);
        assert_eq!(p[11], 0b0000_0001);
    }
}

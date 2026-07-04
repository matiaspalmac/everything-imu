//! Lizard mode disable + watchdog.
//!
//! The kernel `hid-steam` driver puts the Deck into "lizard mode" so the
//! controller acts as a keyboard + mouse in non-Steam contexts. To stream raw
//! HID input reports we must repeatedly send three feature reports.
//!
//! The watchdog must be fed well under ~1 s; we use 500 ms for safety margin.

use std::time::Duration;

/// Steam Controller/Deck feature-report IDs. These are sent as feature reports
/// (HID set_report op) on the gamepad interface.
pub const ID_CLEAR_DIGITAL_MAPPINGS: u8 = 0x81;
pub const ID_LOAD_DEFAULT_SETTINGS: u8 = 0x8E;
pub const ID_SET_DIGITAL_MAPPINGS: u8 = 0x80;

/// Recommended interval between watchdog feedings. The kernel re-enables
/// lizard mode after ~1 s of inactivity; 500 ms keeps us comfortably ahead.
pub const WATCHDOG_INTERVAL: Duration = Duration::from_millis(500);

/// Build a 65-byte feature report (1 byte report ID + 64 byte payload) for the
/// given command. The payload is zero-filled; this matches the
/// "empty digital mapping" request that the kernel interprets as "no key/mouse
/// emulation; pass raw HID through".
pub fn build_feature_report(report_id: u8) -> [u8; 65] {
    let mut out = [0u8; 65];
    out[0] = report_id;
    out
}

/// Full three-step "kill lizard" sequence. Caller sends each in order.
pub fn lizard_disable_sequence() -> [[u8; 65]; 3] {
    [
        build_feature_report(ID_CLEAR_DIGITAL_MAPPINGS),
        build_feature_report(ID_LOAD_DEFAULT_SETTINGS),
        build_feature_report(ID_SET_DIGITAL_MAPPINGS),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_report_size_matches_steam_protocol() {
        // 65 = 1 report-id byte + 64 payload bytes (the Steam Controller /
        // Deck protocol uses fixed-size 64-byte feature payloads).
        assert_eq!(build_feature_report(0x80).len(), 65);
    }

    #[test]
    fn feature_report_carries_report_id_first() {
        let buf = build_feature_report(0x42);
        assert_eq!(buf[0], 0x42);
        assert!(buf[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn lizard_sequence_uses_three_documented_ids() {
        let seq = lizard_disable_sequence();
        assert_eq!(seq[0][0], ID_CLEAR_DIGITAL_MAPPINGS);
        assert_eq!(seq[1][0], ID_LOAD_DEFAULT_SETTINGS);
        assert_eq!(seq[2][0], ID_SET_DIGITAL_MAPPINGS);
    }

    #[test]
    fn watchdog_interval_under_one_second() {
        assert!(WATCHDOG_INTERVAL < Duration::from_millis(1000));
    }
}

//! PS Move USB-time configuration: Bluetooth pairing (feature report 0x05) and
//! factory-calibration read (feature report 0x10).
//!
//! These run only while the controller is tethered over **USB** — the IMU
//! itself streams exclusively over Bluetooth, but pairing and the per-unit cal
//! blob are read/written over the USB HID interface.
//!
//! ## Pairing flow (per `docs/reference/psmove_protocol.md`)
//! 1. Plug the Move in over USB; it enumerates as HID (VID 0x054C).
//! 2. Write the host Bluetooth adapter MAC via feature report 0x05.
//! 3. Unplug USB and press the PS button — the Move reconnects over BT using the
//!    stored host MAC and begins streaming input report 0x01.
//!
//! On Windows 10 1903+ the OS may reject a stale pairing; the user must remove
//! the controller from Bluetooth settings first before re-pairing.

use crate::calibration::{parse_factory_blob, ImuCalibration, CAL_BLOB_LEN, CAL_BLOCK_LEN};
use crate::ids::ControllerKind;
use hidapi::HidDevice;

const REQ_SET_BT_ADDR: u8 = 0x05;
const REQ_GET_CALIBRATION: u8 = 0x10;
const SET_BT_ADDR_LEN: usize = 23;

/// Write the host Bluetooth adapter MAC into the Move's firmware so it will
/// reconnect to this PC over BT. `host_mac` is in wire order (the same byte
/// order the adapter reports it, least-significant first per the ref doc).
///
/// Must be called over the USB HID interface.
pub fn pair_to_host(dev: &HidDevice, host_mac: [u8; 6]) -> Result<(), String> {
    let mut report = [0u8; SET_BT_ADDR_LEN];
    report[0] = REQ_SET_BT_ADDR;
    report[1..7].copy_from_slice(&host_mac);
    // bytes 7..23 stay zero (reserved).
    dev.send_feature_report(&report)
        .map_err(|e| format!("psmove set-bt-addr (0x05) failed: {e}"))?;
    Ok(())
}

/// Read the factory IMU calibration blob (feature report 0x10) and parse it.
///
/// The device splits the blob across two feature reads; this issues both and
/// concatenates them in block order (discriminator at byte 1 of each block)
/// before parsing. Falls back to [`ImuCalibration::identity`] inside the parser
/// if the blob is short or degenerate. Must be called over USB.
pub fn read_factory_calibration(
    dev: &HidDevice,
    kind: ControllerKind,
) -> Result<ImuCalibration, String> {
    let mut blob = vec![0u8; CAL_BLOB_LEN];
    let mut got_block = [false; 2];

    // The two halves can arrive in either order across the two reads; the byte
    // at index 1 of each block identifies it (0 = first half, 1 = second).
    for _ in 0..2 {
        let mut block = [0u8; CAL_BLOCK_LEN];
        block[0] = REQ_GET_CALIBRATION;
        let n = dev
            .get_feature_report(&mut block)
            .map_err(|e| format!("psmove get-calibration (0x10) failed: {e}"))?;
        if n < CAL_BLOCK_LEN {
            return Err(format!(
                "psmove calibration block short: {n} < {CAL_BLOCK_LEN}"
            ));
        }
        let idx = (block[1] & 0x01) as usize;
        blob[idx * CAL_BLOCK_LEN..(idx + 1) * CAL_BLOCK_LEN].copy_from_slice(&block);
        got_block[idx] = true;
    }

    if !got_block[0] || !got_block[1] {
        return Err("psmove calibration: did not receive both blocks".into());
    }
    Ok(parse_factory_blob(kind, &blob))
}

/// Read this host's Bluetooth adapter MAC (wire order, LSB first) for pairing.
///
/// Linux reads the adapter address from sysfs; Windows is validation-pending
/// (the `BluetoothGetRadioInfo` / registry route is documented below) — supply
/// the MAC explicitly there for now.
#[cfg(target_os = "linux")]
pub fn read_host_mac() -> Result<[u8; 6], String> {
    // BlueZ exposes the first adapter at /sys/class/bluetooth/hci0/address as
    // colon-separated MSB-first text ("AA:BB:CC:DD:EE:FF").
    let text = std::fs::read_to_string("/sys/class/bluetooth/hci0/address")
        .map_err(|e| format!("read hci0 address failed: {e}"))?;
    let mut mac = parse_mac_str(text.trim())?;
    // sysfs gives MSB-first; the Move firmware wants wire order (LSB first).
    mac.reverse();
    Ok(mac)
}

#[cfg(target_os = "windows")]
pub fn read_host_mac() -> Result<[u8; 6], String> {
    // Windows route (validation-pending, no BT stack dep wired this session):
    //   1. `BluetoothFindFirstRadio` → handle to the local radio.
    //   2. `BluetoothGetRadioInfo` → `BLUETOOTH_RADIO_INFO.address` (u64 MAC).
    // Until that is implemented, the caller should supply the host MAC and call
    // `pair_to_host` directly.
    Err("host BT MAC auto-read not implemented on Windows; supply it explicitly".into())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn read_host_mac() -> Result<[u8; 6], String> {
    Err("host BT MAC read unsupported on this platform".into())
}

/// Parse a colon- or dash-separated MAC string into 6 bytes (MSB-first, as
/// written).
pub fn parse_mac_str(s: &str) -> Result<[u8; 6], String> {
    let parts: Vec<&str> = s.split([':', '-']).collect();
    if parts.len() != 6 {
        return Err(format!("expected 6 MAC octets, got {}", parts.len()));
    }
    let mut mac = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        mac[i] =
            u8::from_str_radix(p.trim(), 16).map_err(|e| format!("bad MAC octet '{p}': {e}"))?;
    }
    Ok(mac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mac_colon_and_dash() {
        assert_eq!(
            parse_mac_str("AA:BB:CC:DD:EE:FF").unwrap(),
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
        );
        assert_eq!(
            parse_mac_str("01-23-45-67-89-ab").unwrap(),
            [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB]
        );
    }

    #[test]
    fn parse_mac_rejects_wrong_length() {
        assert!(parse_mac_str("AA:BB:CC").is_err());
        assert!(parse_mac_str("ZZ:BB:CC:DD:EE:FF").is_err());
    }
}

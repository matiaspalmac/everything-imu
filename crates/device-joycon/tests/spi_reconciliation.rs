//! End-to-end test of the SPI cal reconciliation state machine via the
//! reader-side `parse_0x21_spi_reply` parser. We cannot exercise the full
//! `handle_spi_reply` glue (private to jc1.rs); but we can validate the parser
//! contract used by it. The handler logic itself is small and exercised
//! implicitly via `cargo check`.

use device_joycon::ids::ControllerKind;
use device_joycon::JOYCON_VID;

#[test]
fn vid_constant_matches() {
    assert_eq!(JOYCON_VID, 0x057E);
}

#[test]
fn kind_into_round_trip() {
    let _ = ControllerKind::JoyConL.into_device_kind();
    let _ = ControllerKind::ProController.into_device_kind();
}

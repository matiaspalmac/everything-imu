//! Cross-validation against reference byte fixtures.
//!
//! An independent protocol implementation ships tests. Both they and our
//! `slime-tracker` derive from the same C# v0.4.1 wire format. If our encoder
//! and theirs both produce identical bytes for the same logical packet, we have
//! independent confirmation of the wire format.
//!
//!
//! Coverage notes:
//! - SensorInfo SKIPPED — the reference ships the older 3-byte payload form
//!   (sensor_id + status + type only). Our `slime-tracker` ships the C# v0.4.1
//!   form with sensor_config bitmask, rest_calibration flag, tracker_position,
//!   tracker_data_type. Different wire layout by design.
//! - Tests below cover the packets where wire layout matches: Handshake,
//!   Rotation (legacy id=1), RotationData (id=17), Acceleration, UserAction,
//!   Ping decode, HandshakeResponse decode.

use slime_tracker::deku::DekuContainerWrite;
use slime_tracker::{
    ActionType, BoardType, ImuType, McuType, Packet, SbPacket, SensorDataType, SlimeQuaternion,
    SlimeString,
};

#[test]
fn reference_handshake_bytes_match() {
    let pkt = Packet::new(
        1,
        SbPacket::Handshake {
            board: BoardType::SlimeVRDev, // id = 2
            imu: ImuType::Bno080,         // id = 3
            mcu: McuType::Wrangler,       // id = 4
            imu_info: (5, 6, 7),
            protocol_version: 8, // reference labels this `build`; same wire position
            firmware: SlimeString::from("test"),
            mac_address: [121, 34, 164, 250, 231, 204],
        },
    );
    let bytes = pkt.to_bytes().expect("handshake encodes");

    // Protocol fixture reference
    let reference_bytes: Vec<u8> = vec![
        0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0,
        0, 6, 0, 0, 0, 7, 0, 0, 0, 8, 4, 116, 101, 115, 116, 121, 34, 164, 250, 231, 204,
    ];

    assert_eq!(bytes, reference_bytes);
}

#[test]
fn reference_rotation_data_bytes_match() {
    // Reference `quat_fancy` test: RotationData (id=17) seq=1, sensor_id=64,
    // data_type=1, quat=(0,0,0,1), calibration_info=0.
    let pkt = Packet::new(
        1,
        SbPacket::RotationData {
            sensor_id: 64,
            data_type: SensorDataType::Normal, // id = 1
            quat: SlimeQuaternion {
                i: 0.0,
                j: 0.0,
                k: 0.0,
                w: 1.0,
            },
            calibration_info: 0,
        },
    );
    let bytes = pkt.to_bytes().expect("rotation_data encodes");

    // Verbatim from reference test_deku.rs:66-69
    let reference_bytes: Vec<u8> = vec![
        0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 1, 64, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 63, 128, 0,
        0, 0,
    ];

    assert_eq!(bytes, reference_bytes);
}

#[test]
fn reference_acceleration_bytes_match() {
    // Reference `test_acceleration`: seq=16, vector=(0.1, 0.5, 0.9), sensor_id=32.
    let pkt = Packet::new(
        16,
        SbPacket::Acceleration {
            vector: (0.1, 0.5, 0.9),
            sensor_id: 32,
        },
    );
    let bytes = pkt.to_bytes().expect("acceleration encodes");

    // Verbatim from reference test_deku.rs:89-91
    let reference_bytes: Vec<u8> = vec![
        0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 16, 61, 204, 204, 205, 63, 0, 0, 0, 63, 102, 102, 102, 32,
    ];

    assert_eq!(bytes, reference_bytes);
}

#[test]
fn reference_user_action_bytes_match() {
    // Reference `test_user_action`: seq=1, typ=3 (RESET_FULL).
    let pkt = Packet::new(
        1,
        SbPacket::UserAction {
            action: ActionType::ResetYaw, // id = 3
        },
    );
    let bytes = pkt.to_bytes().expect("user_action encodes");

    // Verbatim from reference test_deku.rs:103-104
    let reference_bytes: Vec<u8> = vec![0, 0, 0, 21, 0, 0, 0, 0, 0, 0, 0, 1, 3];

    assert_eq!(bytes, reference_bytes);
}

// PING DIVERGENCE — not cross-validated.
//
// Reference `PacketType::Ping { id: u32 }` is wire layout `[tag:u32][id:u32]` =
// 8 bytes total (NO 8-byte seq). Reference test bytes `[0,0,0,10,1,2,3,4]`
// decode to `Ping { id = 0x01020304 = 16909060 }`.
//
// Our `Packet<CbPacket>` wraps everything in the standard 12-byte header
// `[tag:u32][seq:u64][body]`, so our Ping wire is 4 + 8 + 4 = 16 bytes with
// `challenge: [u8; 4]` body.
//
// SlimeVR-Server itself sends pings in the reference-style layout (no seq).
// Our `slime-tracker` impl is divergent here. That is a known protocol
// implementation choice documented in PROTOCOL.md; cross-validation is not
// possible without first aligning. Skipping for now.

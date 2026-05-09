//! Byte-compat tests against reference encoders.
//!
//! Each test asserts that the encoder produces byte-for-byte identical
//! output to reference packets for the same inputs.
//!
//! When real reference captures (via `dumpcap` UDP loopback port 6969) become
//! available, drop the raw `.bin` files into `crates/slime-tracker/fixtures/`
//! and add `assert_eq!(bytes, include_bytes!("../fixtures/<name>.bin"))`-style
//! verifications alongside these synthesized expectations. Both should match.

use slime_tracker::deku::DekuContainerWrite;
use slime_tracker::*;

fn header(tag: u32, seq: u64) -> Vec<u8> {
    let mut h = Vec::with_capacity(12);
    h.extend_from_slice(&tag.to_be_bytes());
    h.extend_from_slice(&seq.to_be_bytes());
    h
}

#[test]
fn handshake_v0_4_1_byte_compat() {
    // Mirrors reference builder for a "Custom" board with
    // a Bmi270 IMU on Esp32 + magnetometer NOT_SUPPORTED + protocol version 19.
    let pkt = Packet::new(
        0,
        SbPacket::Handshake {
            board: BoardType::Custom,
            imu: ImuType::Bmi270,
            mcu: McuType::Esp32,
            // imu_info[0] carries MagnetometerStatus.NOT_SUPPORTED (= 0).
            imu_info: (0, 0, 0),
            protocol_version: 19,
            firmware: SlimeString::from("EverythingIMU 1.0.0"),
            mac_address: [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE],
        },
    );
    let bytes = pkt.to_bytes().expect("handshake encodes");

    let mut expected = header(3, 0);
    expected.extend_from_slice(&4u32.to_be_bytes()); // BoardType::Custom
    expected.extend_from_slice(&11u32.to_be_bytes()); // ImuType::Bmi270 (3-byte pad + u8)
    expected.extend_from_slice(&2u32.to_be_bytes()); // McuType::Esp32
    expected.extend_from_slice(&0i32.to_be_bytes()); // imu_info[0] (mag status)
    expected.extend_from_slice(&0i32.to_be_bytes()); // imu_info[1]
    expected.extend_from_slice(&0i32.to_be_bytes()); // imu_info[2]
    expected.extend_from_slice(&19i32.to_be_bytes()); // protocol_version
    expected.push(19); // firmware id length
    expected.extend_from_slice(b"EverythingIMU 1.0.0");
    expected.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE]);

    assert_eq!(bytes, expected);
}

#[test]
fn sensor_info_no_mag_byte_compat() {
    let pkt = Packet::new(
        1,
        SbPacket::SensorInfo {
            sensor_id: 0,
            sensor_status: SensorStatus::Ok,
            sensor_type: ImuType::Bmi270,
            sensor_config: 0x0000,
            has_completed_rest_calibration: 0,
            tracker_position: TrackerPosition::None,
            tracker_data_type: TrackerDataType::Rotation,
        },
    );
    let bytes = pkt.to_bytes().expect("sensor_info encodes");

    let mut expected = header(15, 1);
    expected.push(0); // sensor_id
    expected.push(1); // sensor_status = Ok (= 1, dashboard turns green)
    expected.push(11); // sensor_type = Bmi270 (1 byte here, vs 4 in handshake)
    expected.extend_from_slice(&0x0000u16.to_be_bytes()); // sensor_config
    expected.push(0); // has_completed_rest_calibration
    expected.push(0); // tracker_position = None
    expected.push(0); // tracker_data_type = Rotation

    assert_eq!(bytes, expected);
}

#[test]
fn sensor_info_with_mag_enabled_byte_compat() {
    // JC2-style 9D fusion path: Lsm6ds3trc + magnetometer enabled
    // (sensor_config = 0x0003 = bit 0 + bit 1).
    let pkt = Packet::new(
        2,
        SbPacket::SensorInfo {
            sensor_id: 1,
            sensor_status: SensorStatus::Ok,
            sensor_type: ImuType::Lsm6ds3trc,
            sensor_config: 0x0003,
            has_completed_rest_calibration: 0,
            tracker_position: TrackerPosition::None,
            tracker_data_type: TrackerDataType::Rotation,
        },
    );
    let bytes = pkt.to_bytes().expect("sensor_info_with_mag encodes");

    let mut expected = header(15, 2);
    expected.push(1); // sensor_id
    expected.push(1); // sensor_status
    expected.push(12); // ImuType::Lsm6ds3trc = 12
    expected.extend_from_slice(&0x0003u16.to_be_bytes());
    expected.push(0);
    expected.push(0);
    expected.push(0);

    assert_eq!(bytes, expected);
}

#[test]
fn rotation_data_byte_compat() {
    // Identity quaternion: i=1, j=0, k=0, w=0. The protocol writes the floats in
    // X, Y, Z, W byte order — same as our (i, j, k, w) field naming.
    let pkt = Packet::new(
        7,
        SbPacket::RotationData {
            sensor_id: 0,
            data_type: SensorDataType::Normal,
            quat: SlimeQuaternion {
                i: 1.0,
                j: 0.0,
                k: 0.0,
                w: 0.0,
            },
            calibration_info: 0,
        },
    );
    let bytes = pkt.to_bytes().expect("rotation_data encodes");

    let mut expected = header(17, 7);
    expected.push(0); // sensor_id
    expected.push(1); // data_type = Normal
    expected.extend_from_slice(&1.0f32.to_be_bytes()); // i (X)
    expected.extend_from_slice(&0.0f32.to_be_bytes()); // j (Y)
    expected.extend_from_slice(&0.0f32.to_be_bytes()); // k (Z)
    expected.extend_from_slice(&0.0f32.to_be_bytes()); // w (W)
    expected.push(0); // calibration_info

    assert_eq!(bytes, expected);
}

#[test]
fn acceleration_byte_compat() {
    // Standard gravity vector along X axis. Note: tracker_id is the LAST byte
    // (after the 12-byte vector), not the first as in rotation_data.
    let pkt = Packet::new(
        42,
        SbPacket::Acceleration {
            vector: (9.806_65, 0.0, 0.0),
            sensor_id: 42,
        },
    );
    let bytes = pkt.to_bytes().expect("acceleration encodes");

    let mut expected = header(4, 42);
    expected.extend_from_slice(&9.806_65f32.to_be_bytes()); // X
    expected.extend_from_slice(&0.0f32.to_be_bytes()); // Y
    expected.extend_from_slice(&0.0f32.to_be_bytes()); // Z
    expected.push(42); // sensor_id (trailing)

    assert_eq!(bytes, expected);
}

#[test]
fn magnetometer_byte_compat() {
    // Reference `BuildMagnetometerPacket` layout: id, dataType=1, x/y/z, calibration.
    let pkt = Packet::new(
        99,
        SbPacket::Magnetometer {
            sensor_id: 0,
            data_type: SensorDataType::Normal,
            vector: (1.5, 2.5, 3.5),
            calibration_info: 0,
        },
    );
    let bytes = pkt.to_bytes().expect("magnetometer encodes");

    let mut expected = header(5, 99);
    expected.push(0); // sensor_id
    expected.push(1); // data_type = Normal
    expected.extend_from_slice(&1.5f32.to_be_bytes());
    expected.extend_from_slice(&2.5f32.to_be_bytes());
    expected.extend_from_slice(&3.5f32.to_be_bytes());
    expected.push(0); // calibration_info

    assert_eq!(bytes, expected);
}

#[test]
fn bundle_2_inners_round_trip_byte_compat() {
    // Inner payloads do NOT include their own seq number — only the outer
    // BUNDLE carries one. The protocol strips bytes [4..12] from each pre-built inner.

    // Rotation body: sensor_id=0, data_type=Normal, identity quat, cal=0.
    let mut rot_payload = Vec::new();
    rot_payload.push(0);
    rot_payload.push(1);
    rot_payload.extend_from_slice(&1.0f32.to_be_bytes());
    rot_payload.extend_from_slice(&0.0f32.to_be_bytes());
    rot_payload.extend_from_slice(&0.0f32.to_be_bytes());
    rot_payload.extend_from_slice(&0.0f32.to_be_bytes());
    rot_payload.push(0);
    assert_eq!(rot_payload.len(), 19);

    // Accel body: vector (0, 0, 9.8), sensor_id=0.
    let mut acc_payload = Vec::new();
    acc_payload.extend_from_slice(&0.0f32.to_be_bytes());
    acc_payload.extend_from_slice(&0.0f32.to_be_bytes());
    acc_payload.extend_from_slice(&9.8f32.to_be_bytes());
    acc_payload.push(0);
    assert_eq!(acc_payload.len(), 13);

    let inners = [
        (17u32, rot_payload.as_slice()),
        (4u32, acc_payload.as_slice()),
    ];
    let bytes = encode_bundle(123, &inners);

    let mut expected = header(BUNDLE_TAG, 123);
    expected.extend_from_slice(&((4 + rot_payload.len()) as u16).to_be_bytes()); // 23
    expected.extend_from_slice(&17u32.to_be_bytes());
    expected.extend_from_slice(&rot_payload);
    expected.extend_from_slice(&((4 + acc_payload.len()) as u16).to_be_bytes()); // 17
    expected.extend_from_slice(&4u32.to_be_bytes());
    expected.extend_from_slice(&acc_payload);

    assert_eq!(bytes, expected);

    // Round-trip through decoder.
    let (decoded_seq, decoded_inners) = decode_bundle(&bytes).expect("bundle decodes");
    assert_eq!(decoded_seq, 123);
    assert_eq!(decoded_inners.len(), 2);
    assert_eq!(decoded_inners[0].0, 17);
    assert_eq!(decoded_inners[0].1, rot_payload.as_slice());
    assert_eq!(decoded_inners[1].0, 4);
    assert_eq!(decoded_inners[1].1, acc_payload.as_slice());
}

#[test]
fn feature_flags_outbound_sensor_config_bit() {
    // Tracker-side advertisement: SENSOR_CONFIG bit (= 2) set, all others zero.
    // Reference output produces a single trailing byte with bit 2 set.
    let pkt = Packet::new(
        11,
        SbPacket::FeatureFlags {
            flag_bytes: vec![1u8 << firmware_feature_flag_bits::SENSOR_CONFIG],
        },
    );
    let bytes = pkt.to_bytes().expect("feature_flags encodes");

    let mut expected = header(22, 11);
    expected.push(0b0000_0100); // bit 2 (SENSOR_CONFIG)

    assert_eq!(bytes, expected);
}

#[test]
fn bundle_decode_rejects_wrong_tag() {
    // Encoder accepts a wrong outer tag if you inject one, but decoder must
    // bail out with WrongTag rather than parse it as bundle inners.
    let mut bogus = header(99, 0);
    bogus.extend_from_slice(&[0, 0]); // dummy bytes
    let err = decode_bundle(&bogus).unwrap_err();
    assert_eq!(err, DeserializeError::WrongTag(99));
}

#[test]
fn bundle_decode_rejects_truncated() {
    // Outer header missing — must produce Truncated.
    assert_eq!(decode_bundle(&[]).unwrap_err(), DeserializeError::Truncated);
    assert_eq!(
        decode_bundle(&[0, 0, 0, 100]).unwrap_err(),
        DeserializeError::Truncated
    );

    // Outer header present, but inner length advertises more bytes than remain.
    let mut buf = header(BUNDLE_TAG, 0);
    buf.extend_from_slice(&100u16.to_be_bytes()); // claims 100 bytes follow
    buf.extend_from_slice(&17u32.to_be_bytes()); // inner_type
                                                 // ...but we only supply 4 more bytes after this
    buf.extend_from_slice(&[0, 0, 0, 0]);
    assert_eq!(
        decode_bundle(&buf).unwrap_err(),
        DeserializeError::Truncated
    );
}

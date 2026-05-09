//! Server-bound packets (tracker → SlimeVR-Server).

use deku::prelude::*;

use crate::{SlimeQuaternion, SlimeString};

/// Packet IDs and layouts — confirmed against C# v0.4.1 `PacketBuilder.cs` +
/// `FirmwareConstants.UDPPackets`. Tag is the first 4 bytes of every datagram
/// (u32 BE), followed by the 8-byte sequence number from the [`crate::Packet`]
/// wrapper.
#[derive(Debug, Clone, PartialEq, DekuRead, DekuWrite)]
#[deku(ctx = "_: deku::ctx::Endian, tag: u32", id = "tag", endian = "big")]
#[non_exhaustive]
pub enum SbPacket {
    /// Heartbeat / keepalive. C# server expects one trailing `u8` tracker id (0).
    #[deku(id = "0")]
    Heartbeat { tracker_id: u8 },

    /// Initial connection packet. Client sends this on broadcast (or the
    /// configured server IP) until receiving a "Hey OVR" handshake reply.
    #[deku(id = "3")]
    Handshake {
        board: BoardType,
        /// IMU type is encoded as 4 bytes in the handshake (3-byte zero pad +
        /// u8 enum value), while [`SbPacket::SensorInfo`] sends the same enum
        /// as a single byte. The C# reference encoder calls
        /// `WriteInt32((int)imuType)` here.
        #[deku(pad_bytes_before = "3")]
        imu: ImuType,
        mcu: McuType,
        /// 3 × i32 IMU info slots. Slot 0 carries [`MagnetometerStatus`] in the
        /// C# v0.4.1 implementation; slots 1 and 2 are reserved/zero.
        imu_info: (i32, i32, i32),
        /// SlimeVR firmware protocol version. C# v0.4.1 sends 19.
        protocol_version: i32,
        firmware: SlimeString,
        mac_address: [u8; 6],
    },

    /// Linear acceleration vector + sensor id (m/s², not g).
    #[deku(id = "4")]
    Acceleration {
        vector: (f32, f32, f32),
        sensor_id: u8,
    },

    /// Magnetometer reading (µT). Sent only when the JC2 9D fusion path is
    /// active. Layout mirrors gyro: id, type=1, x/y/z, calibration.
    #[deku(id = "5")]
    Magnetometer {
        sensor_id: u8,
        data_type: SensorDataType,
        vector: (f32, f32, f32),
        calibration_info: u8,
    },

    /// Server-issued ping echoes back unmodified. Used by SlimeVR-Server's
    /// latency display.
    #[deku(id = "10")]
    Ping { challenge: [u8; 4] },

    /// Battery telemetry. `voltage_volts` defaults to 3.7V if unknown (zero
    /// hides the indicator). `level` is normalized 0.0–1.0.
    #[deku(id = "12")]
    BatteryLevel { voltage_volts: f32, level: f32 },

    /// Per-tracker description packet. Sent right after the handshake reply
    /// for each logical sensor on the device. `sensor_config` carries the
    /// magnetometer-enabled / -supported bitmask (0x0003 enabled, 0x0002
    /// supported-but-disabled, 0x0000 unsupported).
    #[deku(id = "15")]
    SensorInfo {
        sensor_id: u8,
        sensor_status: SensorStatus,
        sensor_type: ImuType,
        sensor_config: u16,
        has_completed_rest_calibration: u8,
        tracker_position: TrackerPosition,
        tracker_data_type: TrackerDataType,
    },

    /// Modern rotation packet. Quaternion in `(X, Y, Z, W)` byte order.
    /// `data_type = 1` ([`SensorDataType::Normal`]) for live samples.
    #[deku(id = "17")]
    RotationData {
        sensor_id: u8,
        data_type: SensorDataType,
        quat: SlimeQuaternion,
        calibration_info: u8,
    },

    /// User action button press (reset yaw, reset full, etc.).
    #[deku(id = "21")]
    UserAction { action: ActionType },

    /// Bidirectional capability advertisement. When the tracker sends it, bits
    /// use the [`FirmwareFeatureFlagBits`] namespace. The server reply (also
    /// id 22) uses [`ServerFeatureFlagBits`] — see the [`crate::CbPacket`]
    /// variant. Tracker-side flag bytes are LSB0-bit-ordered, count is the
    /// number of trailing bytes.
    #[deku(id = "22")]
    FeatureFlags {
        #[deku(read_all)]
        flag_bytes: Vec<u8>,
    },

    /// Tracker→server response to a [`crate::CbPacket::SetConfigFlag`] request.
    /// Echoes the sensor and config IDs so the server can correlate the ack
    /// with its own pending change.
    #[deku(id = "24")]
    AckConfigChange { sensor_id: u8, config_type: u16 },

    /// Multi-packet bundle. Server must have advertised
    /// [`ServerFeatureFlagBits::PROTOCOL_BUNDLE_SUPPORT`] in its FEATURE_FLAGS
    /// reply — otherwise legacy servers silently drop the datagram. Each
    /// inner is `[u16 BE length][u32 BE inner_type][inner_payload]` — note
    /// inner packets do NOT include their own sequence number; only the
    /// outer one does.
    ///
    /// Layout is hand-rolled because deku has no built-in for "bytes until
    /// end of buffer". Use [`encode_bundle`] / [`decode_bundle`] helpers.
    #[deku(id = "100")]
    Bundle {
        #[deku(read_all)]
        inner: Vec<u8>,
    },
}

/// The board design for a SlimeVR tracker. Values match the C#
/// `FirmwareConstants.BoardType` enum.
#[derive(Debug, Clone, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(id_type = "u32", ctx = "_: deku::ctx::Endian", endian = "big")]
#[non_exhaustive]
pub enum BoardType {
    #[deku(id = "0")]
    Unknown,
    #[deku(id = "1")]
    SlimeVRLegacy,
    #[deku(id = "2")]
    SlimeVRDev,
    #[deku(id = "3")]
    NodeMCU,
    /// `BoardType::Custom = 4` — used by the C# v0.4.1 bridge to identify
    /// itself to SlimeVR-Server as a "custom" tracker source.
    #[deku(id = "4")]
    Custom,
    #[deku(id = "5")]
    WRoom32,
    #[deku(id = "6")]
    WemosD1Mini,
    #[deku(id = "7")]
    TTGOTBase,
    #[deku(id = "8")]
    ESP01,
    #[deku(id = "9")]
    SlimeVR,
    #[deku(id = "10")]
    LolinC3Mini,
    #[deku(id = "11")]
    Beetle32C3,
    #[deku(id = "12")]
    ESP32C3DevKitM1,
    #[deku(id = "13")]
    OwoTrack,
    #[deku(id = "14")]
    Wrangler,
    #[deku(id = "15")]
    Mocopi,
    #[deku(id = "16")]
    WemosWroom02,
    #[deku(id = "17")]
    XiaoEsp32C3,
    #[deku(id = "18")]
    Haritora,
    #[deku(id = "250")]
    DevReserved,
    #[deku(id_pat = "_")]
    UnknownVariant(u32),
}

/// The IMU chip in use. Sent in handshake (4-byte i32-padded form per the
/// C# v0.4.1 layout) and in [`SbPacket::SensorInfo`] (1-byte form).
#[derive(Debug, PartialEq, Eq, Clone, Copy, DekuRead, DekuWrite)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian", endian = "big")]
#[non_exhaustive]
pub enum ImuType {
    #[deku(id = "0")]
    Unknown,
    #[deku(id = "1")]
    Mpu9250,
    #[deku(id = "2")]
    Mpu6500,
    #[deku(id = "3")]
    Bno080,
    #[deku(id = "4")]
    Bno085,
    #[deku(id = "5")]
    Bno055,
    #[deku(id = "6")]
    Mpu6050,
    #[deku(id = "7")]
    Bno086,
    #[deku(id = "8")]
    Bmi160,
    #[deku(id = "9")]
    Icm20948,
    #[deku(id = "10")]
    Icm42688,
    #[deku(id = "11")]
    Bmi270,
    #[deku(id = "12")]
    Lsm6ds3trc,
    #[deku(id = "13")]
    Lsm6dsv,
    #[deku(id = "14")]
    Lsm6dso,
    #[deku(id = "15")]
    Lsm6dsr,
    #[deku(id = "16")]
    Icm45686,
    #[deku(id = "17")]
    Icm45605,
    #[deku(id = "18")]
    AdcResistance,
    #[deku(id = "250")]
    DevReserved,
    #[deku(id_pat = "_")]
    UnknownVariant(u8),
}

/// Microcontroller family.
#[derive(Debug, Clone, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(id_type = "u32", ctx = "_: deku::ctx::Endian", endian = "big")]
#[non_exhaustive]
pub enum McuType {
    #[deku(id = "0")]
    Unknown,
    #[deku(id = "1")]
    Esp8266,
    #[deku(id = "2")]
    Esp32,
    #[deku(id = "3")]
    OwoTrackAndroid,
    #[deku(id = "4")]
    Wrangler,
    #[deku(id = "5")]
    OwoTrackIos,
    #[deku(id = "6")]
    Esp32C3,
    #[deku(id = "7")]
    Mocopi,
    #[deku(id = "8")]
    Haritora,
    #[deku(id = "250")]
    DevReserved,
    #[deku(id_pat = "_")]
    UnknownVariant(u32),
}

/// Sensor connection state. SlimeVR-Server maps `0` to DISCONNECTED and `1` to
/// OK — C# v0.4.1 always sends `1` after registration so the dashboard goes
/// green immediately.
#[derive(Debug, Clone, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian", endian = "big")]
pub enum SensorStatus {
    #[deku(id = "0")]
    Offline,
    #[deku(id = "1")]
    Ok,
}

/// How a [`SbPacket::RotationData`] / [`SbPacket::Magnetometer`] payload should
/// be interpreted. `Normal = 1` is the only value sent by C# v0.4.1.
#[derive(Debug, Clone, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian", endian = "big")]
pub enum SensorDataType {
    #[deku(id = "1")]
    /// Live sensor sample.
    Normal,
    #[deku(id = "2")]
    /// Correction offset. Never sent by the C++ firmware or our bridge.
    Correction,
}

/// User action button events sent via [`SbPacket::UserAction`].
#[derive(Debug, Clone, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian", endian = "big")]
#[non_exhaustive]
pub enum ActionType {
    #[deku(id = "2")]
    ResetFull,
    #[deku(id = "3")]
    ResetYaw,
    #[deku(id = "4")]
    ResetMounting,
    #[deku(id = "5")]
    PauseTracking,
    #[deku(id_pat = "_")]
    UnknownVariant(u8),
}

/// Tracker body position published in [`SbPacket::SensorInfo`]. Values match
/// SlimeVR-Server's `TrackerPosition` enum.
#[derive(Debug, Clone, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian", endian = "big")]
#[non_exhaustive]
pub enum TrackerPosition {
    #[deku(id = "0")]
    None,
    #[deku(id = "1")]
    Head,
    #[deku(id = "2")]
    Neck,
    #[deku(id = "3")]
    UpperChest,
    #[deku(id = "4")]
    Chest,
    #[deku(id = "5")]
    Waist,
    #[deku(id = "6")]
    Hip,
    #[deku(id = "7")]
    LeftUpperLeg,
    #[deku(id = "8")]
    RightUpperLeg,
    #[deku(id = "9")]
    LeftLowerLeg,
    #[deku(id = "10")]
    RightLowerLeg,
    #[deku(id = "11")]
    LeftFoot,
    #[deku(id = "12")]
    RightFoot,
    #[deku(id_pat = "_")]
    UnknownVariant(u8),
}

/// Tracker data classification. Matches SlimeVR-Server's `TrackerDataType`.
#[derive(Debug, Clone, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian", endian = "big")]
pub enum TrackerDataType {
    #[deku(id = "0")]
    Rotation,
    #[deku(id = "1")]
    FlexResistance,
    #[deku(id = "2")]
    FlexAngle,
}

/// Tracker→server bits packed into the FEATURE_FLAGS byte stream. Indices
/// match SlimeVR-Server's `FirmwareFeatures.FirmwareFeatureFlags` enum and
/// must NOT be confused with [`ServerFeatureFlagBits`].
pub mod firmware_feature_flag_bits {
    pub const REMOTE_COMMAND: u8 = 0;
    pub const B64_WIFI_SCANNING: u8 = 1;
    pub const SENSOR_CONFIG: u8 = 2;
}

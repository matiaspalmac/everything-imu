//! Client-bound packets (SlimeVR-Server → tracker).
//!
//! See module docs in [`crate`] for the wire-level header layout (`u32 BE tag +
//! u64 BE seq`).

use deku::prelude::*;

#[derive(Debug, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(ctx = "_: deku::ctx::Endian, tag: u32", id = "tag", endian = "big")]
#[non_exhaustive]
pub enum CbPacket {
    /// Server discovery beacon. Rarely seen in practice — most servers reply
    /// directly to a tracker handshake instead.
    #[deku(id = "0")]
    Discovery,

    /// Server-issued heartbeat. Tracker is expected to echo a heartbeat back
    /// (id 0 in the server-bound namespace).
    #[deku(id = "1")]
    Heartbeat,

    /// Server-issued ping. Tracker must echo the four challenge bytes back as
    /// [`crate::SbPacket::Ping`].
    #[deku(id = "10")]
    Ping { challenge: [u8; 4] },

    /// Server FEATURE_FLAGS advertisement. Same packet id (22) as the
    /// outbound variant, but the trailing flag bytes use the
    /// [`server_feature_flag_bits`] namespace — bit 0 = bundle support,
    /// bit 1 = bundle-compact support.
    #[deku(id = "22")]
    FeatureFlags {
        #[deku(read_all)]
        flag_bytes: Vec<u8>,
    },

    /// Server-issued config toggle (e.g. magnetometer enable/disable). The
    /// tracker should respond with [`crate::SbPacket::AckConfigChange`] for
    /// the same `(sensor_id, config_type)` pair.
    #[deku(id = "25")]
    SetConfigFlag {
        sensor_id: u8,
        config_type: u16,
        state: u8,
    },

    /// Handshake reply. The C# v0.4.1 client identifies these by the leading
    /// 4 bytes spelling `\x03Hey` (`u32::from_be_bytes([3, b'H', b'e', b'y']) =
    /// 0x03486579 = 55_076_217`). The server may append a version byte and the
    /// magic suffix " OVR =D 5"; we just store what's there.
    #[deku(id = "55076217")]
    HandshakeResponse {
        #[deku(read_all)]
        payload: Vec<u8>,
    },
}

/// Server→tracker bits packed in the inbound FEATURE_FLAGS reply
/// ([`CbPacket::FeatureFlags`]). LSB0 bit order, byte 0 of `flag_bytes`.
///
/// Indices match SlimeVR-Server's `ServerFeatureFlags` enum and must NOT be
/// confused with the firmware-side bits in
/// [`crate::firmware_feature_flag_bits`].
pub mod server_feature_flag_bits {
    /// Server can decode [`crate::SbPacket::Bundle`] (packet 100). Default
    /// **false** until the reply lands — otherwise legacy servers silently
    /// drop type-100 packets, leaving handshake-OK trackers with invisible
    /// data.
    pub const PROTOCOL_BUNDLE_SUPPORT: u8 = 0;
    /// Server can decode the bandwidth-halving `BUNDLE_COMPACT` (packet 101).
    pub const PROTOCOL_BUNDLE_COMPACT_SUPPORT: u8 = 1;
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn body_bytes(p: CbPacket) -> Vec<u8> {
        let bytes = Packet::new(0, p).to_bytes().unwrap();
        bytes[12..].to_vec()
    }

    #[test]
    fn discovery_has_empty_body() {
        assert!(body_bytes(CbPacket::Discovery).is_empty());
    }

    #[test]
    fn ping_round_trips_challenge() {
        assert_eq!(
            body_bytes(CbPacket::Ping {
                challenge: [1, 2, 3, 4]
            }),
            &[1, 2, 3, 4]
        );
    }
}

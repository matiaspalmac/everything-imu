//! SlimeVR UDP wire protocol — encoder/decoder.
//!
//! Extended with v0.4.1 features:
//! - BUNDLE (packet 100) + auto-fallback gating on FEATURE_FLAGS reply
//! - FEATURE_FLAGS (packet 22) bidirectional with separate firmware/server bit namespaces
//! - Magnetometer (packet 5), Battery (packet 12), ROTATION_AND_ACCELERATION_COMPACT (23)
//! - SET_CONFIG_FLAG (25) inbound + ACK_CONFIG_CHANGE (24) outbound
//!
//! Wire format: every datagram is a [`Packet<D>`] with `[u32 BE tag][u64 BE seq][D body]`.
//! All values big-endian.
//!
//! The `client` feature (enabled by default) pulls in tokio and exposes the
//! [`client::SlimeClient`] state machine with BUNDLE auto-fallback gating.

#[cfg(feature = "client")]
pub mod client;
mod clientbound;
mod serverbound;

pub use clientbound::*;
pub use deku;
use deku::ctx::Endian;
pub use serverbound::*;

use std::string::FromUtf8Error;

use deku::prelude::*;

/// Quaternion as transmitted on the SlimeVR wire — four big-endian f32 values in
/// `(i, j, k, w)` order (matches C# `System.Numerics.Quaternion` field order
/// `X, Y, Z, W`; the names just differ).
#[derive(Debug, Clone, Copy, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "e", ctx = "e: deku::ctx::Endian")]
pub struct SlimeQuaternion {
    pub i: f32,
    pub j: f32,
    pub k: f32,
    pub w: f32,
}

#[cfg(feature = "nalgebra")]
mod nalgebra_impls {
    use super::SlimeQuaternion;
    use nalgebra::Quaternion;

    impl From<Quaternion<f32>> for SlimeQuaternion {
        fn from(q: Quaternion<f32>) -> Self {
            Self {
                i: q.i,
                j: q.j,
                k: q.k,
                w: q.w,
            }
        }
    }

    impl From<SlimeQuaternion> for Quaternion<f32> {
        fn from(q: SlimeQuaternion) -> Self {
            Self::new(q.w, q.i, q.j, q.k)
        }
    }
}

/// Length-prefixed UTF-8 string used in the SlimeVR handshake. The length field
/// is `u8`, so payloads above 255 bytes must be truncated by the caller.
#[derive(PartialEq, Eq, Debug, Clone, DekuRead, DekuWrite)]
#[deku(endian = "e", ctx = "e: deku::ctx::Endian")]
pub struct SlimeString {
    #[deku(update = "self.data.len()")]
    count: u8,
    #[deku(count = "count")]
    data: Vec<u8>,
}

impl From<&str> for SlimeString {
    fn from(s: &str) -> Self {
        let bytes = s.as_bytes();
        Self {
            count: bytes.len() as _,
            data: bytes.to_vec(),
        }
    }
}

impl From<String> for SlimeString {
    fn from(s: String) -> Self {
        let bytes = s.into_bytes();
        Self {
            count: bytes.len() as _,
            data: bytes,
        }
    }
}

impl SlimeString {
    #[allow(dead_code)]
    fn to_string(&self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.data.clone())
    }
}

/// Generic SlimeVR packet wrapper: `[tag: u32 BE][seq: u64 BE][data: D]`.
///
/// `tag` identifies the variant (matches the integer packet IDs in
/// `crates/slime-tracker` documentation). `seq` is a per-tracker monotonically
/// increasing sequence number, used by the server to drop out-of-order packets.
#[derive(Debug, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct Packet<D>
where
    for<'a> D: DekuReader<'a, (Endian, u32)> + DekuWriter<(Endian, u32)>,
{
    tag: u32,
    seq: u64,
    #[deku(ctx = "*tag")]
    data: D,
}

impl<D> Packet<D>
where
    for<'a> D:
        DekuReader<'a, (Endian, u32)> + DekuWriter<(Endian, u32)> + DekuEnumExt<'static, u32>,
{
    pub fn new(seq: u64, data: D) -> Self {
        Self {
            tag: data.deku_id().unwrap(),
            seq,
            data,
        }
    }

    /// Serialize the packet into `buf`, returning the number of bytes written.
    /// Errors if `buf` is too small or the data could not be encoded.
    pub fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, SerializeError> {
        let bytes = self.to_bytes()?;
        if bytes.len() > buf.len() {
            return Err(SerializeError::BufferTooSmall);
        }
        buf[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }

    pub fn deserialize_from(buf: &[u8]) -> Result<Self, DeserializeError> {
        match Packet::from_bytes((buf, 0)) {
            Ok(((tail, _tail_offset), packet)) => {
                if tail.is_empty() {
                    Ok(packet)
                } else {
                    Err(DeserializeError::BytesRemaining)
                }
            }
            Err(deku) => Err(DeserializeError::Deku(deku)),
        }
    }

    /// Consume the packet and return `(seq, data)`.
    pub fn split(self) -> (u64, D) {
        (self.seq, self.data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerializeError {
    Deku(::deku::DekuError),
    BufferTooSmall,
}

impl From<::deku::DekuError> for SerializeError {
    fn from(deku: ::deku::DekuError) -> Self {
        Self::Deku(deku)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeserializeError {
    Deku(::deku::DekuError),
    /// Unexpectedly had bytes remaining after deserialization.
    BytesRemaining,
    /// Buffer ran out before the expected number of bytes was consumed.
    Truncated,
    /// The outer packet tag did not match the expected value.
    WrongTag(u32),
}

impl From<::deku::DekuError> for DeserializeError {
    fn from(deku: ::deku::DekuError) -> Self {
        Self::Deku(deku)
    }
}

/// Outer packet tag for [`SbPacket::Bundle`] / [`encode_bundle`] / [`decode_bundle`].
pub const BUNDLE_TAG: u32 = 100;

/// Encode a BUNDLE packet (outer tag 100) from a sequence of `(inner_type, payload)`
/// pairs. The C# v0.4.1 reference encoder strips the 8-byte sequence number from
/// each pre-built inner packet before copying — so callers should pass payloads
/// **without** their inner sequence numbers (only the outer one applies).
///
/// Inner format on the wire: `[u16 BE inner_len][u32 BE inner_type][payload]`,
/// where `inner_len = 4 + payload.len()`.
pub fn encode_bundle(seq: u64, inners: &[(u32, &[u8])]) -> Vec<u8> {
    let mut total = 4 + 8;
    for (_, payload) in inners {
        total += 2 + 4 + payload.len();
    }
    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&BUNDLE_TAG.to_be_bytes());
    buf.extend_from_slice(&seq.to_be_bytes());
    for (inner_type, payload) in inners {
        let inner_len = 4 + payload.len();
        debug_assert!(
            inner_len <= u16::MAX as usize,
            "BUNDLE inner length must fit in u16"
        );
        buf.extend_from_slice(&(inner_len as u16).to_be_bytes());
        buf.extend_from_slice(&inner_type.to_be_bytes());
        buf.extend_from_slice(payload);
    }
    buf
}

/// One inner entry inside a [`decode_bundle`] result: `(inner_type, payload_slice)`.
pub type BundleInner<'a> = (u32, &'a [u8]);

/// Output of [`decode_bundle`]: the outer sequence number and the list of
/// inner `(type, payload)` pairs (zero-copy borrow from the input buffer).
pub type DecodedBundle<'a> = (u64, Vec<BundleInner<'a>>);

/// Decode a BUNDLE datagram into the outer sequence number and a list of
/// `(inner_type, payload_slice)` pairs. Slices borrow from `buf`, so no
/// allocation per inner.
pub fn decode_bundle(buf: &[u8]) -> Result<DecodedBundle<'_>, DeserializeError> {
    if buf.len() < 12 {
        return Err(DeserializeError::Truncated);
    }
    let tag = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if tag != BUNDLE_TAG {
        return Err(DeserializeError::WrongTag(tag));
    }
    let seq_bytes: [u8; 8] = buf[4..12].try_into().expect("8-byte slice fits in [u8; 8]");
    let seq = u64::from_be_bytes(seq_bytes);

    let mut cursor = 12usize;
    let mut out = Vec::new();
    while cursor < buf.len() {
        if buf.len() < cursor + 6 {
            return Err(DeserializeError::Truncated);
        }
        let inner_len = u16::from_be_bytes([buf[cursor], buf[cursor + 1]]) as usize;
        cursor += 2;
        if inner_len < 4 || buf.len() < cursor + inner_len {
            return Err(DeserializeError::Truncated);
        }
        let inner_type = u32::from_be_bytes([
            buf[cursor],
            buf[cursor + 1],
            buf[cursor + 2],
            buf[cursor + 3],
        ]);
        cursor += 4;
        let payload_len = inner_len - 4;
        let payload = &buf[cursor..cursor + payload_len];
        cursor += payload_len;
        out.push((inner_type, payload));
    }
    Ok((seq, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dummy packet body used to exercise the [`Packet`] wrapper without depending
    /// on the real `SbPacket` / `CbPacket` enums.
    #[derive(Debug, PartialEq, Eq, DekuRead, DekuWrite)]
    #[deku(ctx = "_: deku::ctx::Endian, tag: u32", id = "tag", endian = "big")]
    enum Dummy {
        #[deku(id = "0")]
        D0,
        #[deku(id = "1")]
        D1,
        #[deku(id = "2")]
        D2 { val: u32 },
    }

    #[test]
    fn packet_d0() {
        for i in 0..10 {
            let packet = Packet::new(i, Dummy::D0);
            let bytes = packet.to_bytes().unwrap();
            #[rustfmt::skip]
            let expected = [
                0, 0, 0, 0, // Variant
                0, 0, 0, 0, 0, 0, 0, i as u8, // Sequence
            ];
            assert_eq!(bytes, expected);
            assert_eq!(
                Packet::from_bytes((&bytes, 0)),
                Ok((([].as_slice(), 0), packet))
            );
        }
    }

    #[test]
    fn packet_d2_value() {
        for i in 0..10 {
            let packet = Packet::new(i, Dummy::D2 { val: i as u32 + 20 });
            let bytes = packet.to_bytes().unwrap();
            #[rustfmt::skip]
            let expected = [
                0, 0, 0, 2, // Variant
                0, 0, 0, 0, 0, 0, 0, i as u8, // Sequence
                0, 0, 0, i as u8 + 20, // Data
            ];
            assert_eq!(bytes, expected);
            assert_eq!(
                Packet::from_bytes((&bytes, 0)),
                Ok((([].as_slice(), 0), packet))
            );
        }
    }
}

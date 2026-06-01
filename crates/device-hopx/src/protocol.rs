//! Pure wire-protocol logic for the BLE IMU tracker.
//!
//! Everything here is transport-free and deterministic so it can be unit-tested
//! without a Bluetooth adapter: the GATT identifiers, the stream start/stop
//! commands, the raw→SI sensor scales, advertised-name matching, the streaming
//! record parser, and the body-frame axis remap.

use std::f32::consts::PI;
use uuid::{uuid, Uuid};

/// Nordic UART Service. Present in the advertisement and used to confirm the
/// tracker exposes the expected GATT layout after connecting.
pub const NUS_SERVICE_UUID: Uuid = uuid!("6e400001-b5a3-f393-e0a9-e50e24dcca9e");
/// NUS TX characteristic — device → host notifications carrying IMU records.
pub const NUS_TX_UUID: Uuid = uuid!("6e400003-b5a3-f393-e0a9-e50e24dcca9e");
/// NUS RX characteristic — host → device command writes (start/stop streaming).
pub const NUS_RX_UUID: Uuid = uuid!("6e400002-b5a3-f393-e0a9-e50e24dcca9e");

/// Command that starts the IMU notification stream.
pub const START_CMD: [u8; 8] = [0x20, 0x10, 0x00, 0xD0, 0x07, 0x34, 0x00, 0x03];
/// Command that stops the IMU notification stream.
pub const STOP_CMD: [u8; 7] = [0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

/// Advertised local-name prefix shared by every unit (e.g. `"Triki 257739387"`).
pub const NAME_PREFIX: &str = "Triki";

/// Standard gravity — project convention sends acceleration in m/s² (not g).
const STANDARD_GRAVITY: f32 = 9.806_65;
/// Accelerometer scale: int16 LSB → m/s² at the ±16 g full-scale range
/// (2048 LSB per g).
pub const ACCEL_M_S2_PER_LSB: f32 = STANDARD_GRAVITY / 2048.0;
/// Gyroscope scale: int16 LSB → rad/s. The IMU is an ST LSM6DS-family part
/// (package "SF" marking) configured at ±2000 dps → 70 mdps/LSB. Confirmed by
/// integrating a measured 90° rotation across three hardware captures (read
/// ~91–95° at this scale). The "too slow" reports earlier were the assumed
/// sample rate (200 Hz) vs the real 52 Hz ODR, not the scale — both are now
/// set correctly.
pub const GYRO_RAD_S_PER_LSB: f32 = 0.070 * (PI / 180.0);

const FRAME_MARKER: u8 = 0x22;
/// A second record marker the firmware interleaves into the stream. We treat it
/// as a valid record boundary for resynchronisation but do not yet decode its
/// payload (see crate docs — needs hardware capture to characterise).
const ALT_MARKER: u8 = 0x25;
const RECORD_LEN: usize = 14;

/// One decoded IMU record in SI units, body frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedRecord {
    pub seq: u8,
    /// Accelerometer, m/s².
    pub accel: [f32; 3],
    /// Gyroscope, rad/s.
    pub gyro: [f32; 3],
}

/// True when a BLE advertised name belongs to one of these trackers.
pub fn name_matches(name: &str) -> bool {
    name.starts_with(NAME_PREFIX)
}

/// Extract the per-unit serial from the advertised name
/// (`"Triki 257739387"` → `"257739387"`). Returns `None` when no serial suffix
/// is present.
pub fn serial_from_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix(NAME_PREFIX)?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

/// Map raw sensor axes into the tracker's body frame.
///
/// Defaults to identity. The accel/gyro region split is handled in
/// `parse_record` (gyro-first wire order, confirmed on hardware). This hook is
/// the single place to apply any further body-frame correction — axis swaps or
/// sign flips — if a tester reports a consistent orientation offset.
pub fn remap_axes(v: [f32; 3]) -> [f32; 3] {
    v
}

/// Reassembles fixed 14-byte IMU records from the notification byte stream.
///
/// Records can span notification boundaries, so bytes are buffered until a full
/// record framed by [`FRAME_MARKER`] is available.
#[derive(Default)]
pub struct RecordParser {
    buf: Vec<u8>,
}

impl RecordParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw notification bytes; returns every complete record now decodable,
    /// scaled to SI units.
    pub fn feed(&mut self, data: &[u8]) -> Vec<ParsedRecord> {
        self.drain_chunks(data)
            .iter()
            .map(|c| parse_record(c))
            .collect()
    }

    /// Like [`RecordParser::feed`], but returns the undecoded int16 channels in
    /// wire order. For hardware characterisation, where applying the (possibly
    /// wrong) scale or axis mapping would bias the captured data.
    pub fn feed_raw(&mut self, data: &[u8]) -> Vec<RawRecord> {
        self.drain_chunks(data).iter().map(parse_raw).collect()
    }

    /// Pull every complete 14-byte record currently buffered, handling
    /// notification-boundary splits and false-marker resync.
    fn drain_chunks(&mut self, data: &[u8]) -> Vec<[u8; RECORD_LEN]> {
        self.buf.extend_from_slice(data);
        let mut chunks = Vec::new();
        loop {
            let Some(idx) = self.buf.iter().position(|&b| b == FRAME_MARKER) else {
                // No marker anywhere — drop the buffer, nothing recoverable.
                self.buf.clear();
                break;
            };
            if idx > 0 {
                self.buf.drain(0..idx);
            }
            if self.buf.len() < RECORD_LEN {
                break;
            }
            // The byte after a real record must be the next record's marker.
            // If it is not, this 0x22 was payload, not a frame start — step one
            // byte forward and rescan.
            if self.buf.len() > RECORD_LEN {
                let next = self.buf[RECORD_LEN];
                if next != FRAME_MARKER && next != ALT_MARKER {
                    self.buf.drain(0..1);
                    continue;
                }
            }
            let mut chunk = [0u8; RECORD_LEN];
            chunk.copy_from_slice(&self.buf[..RECORD_LEN]);
            chunks.push(chunk);
            self.buf.drain(0..RECORD_LEN);
        }
        chunks
    }
}

/// Undecoded record: the six int16 channels in wire order (offsets 2,4,6,8,10,12).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawRecord {
    pub seq: u8,
    pub channels: [i16; 6],
}

fn parse_raw(chunk: &[u8; RECORD_LEN]) -> RawRecord {
    let rd = |o: usize| i16::from_le_bytes([chunk[o], chunk[o + 1]]);
    RawRecord {
        seq: chunk[1],
        channels: [rd(2), rd(4), rd(6), rd(8), rd(10), rd(12)],
    }
}

fn parse_record(chunk: &[u8]) -> ParsedRecord {
    let rd = |o: usize| i16::from_le_bytes([chunk[o], chunk[o + 1]]) as f32;
    // Wire order is gyro-first (offsets 2/4/6), accel second (8/10/12) —
    // confirmed on hardware: rotating the unit moves the first triple, shaking
    // it moves the second. Axes are taken straight (aligned gyro/accel frame);
    // any residual body-frame correction goes through `remap_axes`.
    let gyro = remap_axes([
        rd(2) * GYRO_RAD_S_PER_LSB,
        rd(4) * GYRO_RAD_S_PER_LSB,
        rd(6) * GYRO_RAD_S_PER_LSB,
    ]);
    let accel = remap_axes([
        rd(8) * ACCEL_M_S2_PER_LSB,
        rd(10) * ACCEL_M_S2_PER_LSB,
        rd(12) * ACCEL_M_S2_PER_LSB,
    ]);
    ParsedRecord {
        seq: chunk[1],
        gyro,
        accel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// Build a wire record: `[marker][seq][gx gy gz][ax ay az]` (i16 LE).
    /// Wire order is gyro-first, accel-second — confirmed on hardware:
    /// rotating the unit moves the first triple, shaking it moves the second.
    fn record(seq: u8, gyro: [i16; 3], accel: [i16; 3]) -> Vec<u8> {
        let mut v = vec![FRAME_MARKER, seq];
        for x in gyro.iter().chain(accel.iter()) {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v
    }

    #[test]
    fn start_stop_command_bytes_are_stable() {
        assert_eq!(START_CMD, [0x20, 0x10, 0x00, 0xD0, 0x07, 0x34, 0x00, 0x03]);
        assert_eq!(STOP_CMD, [0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn accel_scale_maps_full_scale_step_to_gravity() {
        assert_relative_eq!(2048.0 * ACCEL_M_S2_PER_LSB, 9.806_65, epsilon = 1e-4);
    }

    #[test]
    fn gyro_scale_is_lsm6ds_2000dps() {
        // ST LSM6DS family at +-2000 dps = 70 mdps/LSB. Confirmed by integrating
        // a measured 90 deg rotation across three hardware captures (read
        // ~91-95 deg at this scale; every other candidate scale missed badly).
        let expected = 0.070_f32.to_radians();
        assert_relative_eq!(GYRO_RAD_S_PER_LSB, expected, epsilon = 1e-6);
    }

    #[test]
    fn name_matches_triki_units_only() {
        assert!(name_matches("Triki 257739387"));
        assert!(name_matches("Triki"));
        assert!(!name_matches("JoyCon2-AA:BB"));
        assert!(!name_matches("triki"));
        assert!(!name_matches(""));
    }

    #[test]
    fn serial_from_name_extracts_unit_number() {
        assert_eq!(
            serial_from_name("Triki 257739387"),
            Some("257739387".to_string())
        );
        assert_eq!(serial_from_name("Triki"), None);
        assert_eq!(serial_from_name("nope"), None);
    }

    #[test]
    fn remap_axes_defaults_to_identity() {
        assert_eq!(remap_axes([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn accel_decodes_straight_from_second_triple() {
        // Gravity lives in the accel vector — the second triple, straight x,y,z.
        let bytes = record(7, [0, 0, 0], [2048, 0, 0]);
        let mut p = RecordParser::new();
        let recs = p.feed(&bytes);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].seq, 7);
        assert_relative_eq!(recs[0].accel[0], 9.806_65, epsilon = 1e-3);
        assert_eq!(recs[0].accel[1], 0.0);
        assert_eq!(recs[0].gyro, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn gyro_decodes_straight_from_first_triple() {
        // Rotation lives in the gyro vector — the first triple, straight x,y,z.
        let bytes = record(2, [1000, 0, 0], [0, 0, 0]);
        let mut p = RecordParser::new();
        let recs = p.feed(&bytes);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].accel, [0.0, 0.0, 0.0]);
        assert_relative_eq!(recs[0].gyro[0], 1000.0 * GYRO_RAD_S_PER_LSB, epsilon = 1e-6);
        assert_eq!(recs[0].gyro[1], 0.0);
    }

    #[test]
    fn reassembles_record_split_across_feeds() {
        let bytes = record(3, [10, -20, 30], [40, -50, 60]);
        let (a, b) = bytes.split_at(5);
        let mut p = RecordParser::new();
        assert!(p.feed(a).is_empty());
        let recs = p.feed(b);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].seq, 3);
    }

    #[test]
    fn discards_leading_garbage_before_marker() {
        let mut bytes = vec![0x00, 0xFF, 0x13];
        bytes.extend_from_slice(&record(9, [1, 2, 3], [4, 5, 6]));
        let mut p = RecordParser::new();
        let recs = p.feed(&bytes);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].seq, 9);
    }

    #[test]
    fn parses_two_back_to_back_records() {
        let mut bytes = record(1, [1, 0, 0], [0, 0, 0]);
        bytes.extend_from_slice(&record(2, [0, 1, 0], [0, 0, 0]));
        let mut p = RecordParser::new();
        let recs = p.feed(&bytes);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].seq, 1);
        assert_eq!(recs[1].seq, 2);
    }

    #[test]
    fn feed_raw_returns_int16_channels_in_wire_order() {
        // Channels are the six int16 at offsets 2,4,6,8,10,12 — undecoded, no
        // scale or axis mapping, so a tester's capture is unbiased.
        let bytes = record(4, [11, 22, 33], [44, 55, 66]);
        let mut p = RecordParser::new();
        let recs = p.feed_raw(&bytes);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].seq, 4);
        // record() lays out gyro triple first, accel triple second.
        assert_eq!(recs[0].channels, [11, 22, 33, 44, 55, 66]);
    }

    #[test]
    fn resyncs_on_false_marker() {
        let mut bytes = vec![FRAME_MARKER];
        bytes.extend_from_slice(&[0xAA; 13]);
        bytes.push(0x00);
        bytes.extend_from_slice(&record(5, [0, 0, 0], [0, 0, 0]));
        let mut p = RecordParser::new();
        let recs = p.feed(&bytes);
        assert!(recs.iter().any(|r| r.seq == 5));
    }
}

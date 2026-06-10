use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceMetadata, ImuSample,
};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// One decoded 12-byte UDP packet: raw 6-axis IMU, little-endian on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreeDsPacket {
    /// Accelerometer X/Y/Z (`hidAccelRead`, raw `i16`).
    pub accel: [i16; 3],
    /// Gyroscope X/Y/Z (`hidGyroRead`, raw `i16`).
    pub gyro: [i16; 3],
}

impl ThreeDsPacket {
    pub const WIRE_LEN: usize = 12;

    /// Parse a wire packet. Returns `None` unless exactly 12 bytes.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() != Self::WIRE_LEN {
            return None;
        }
        let rd = |o: usize| i16::from_le_bytes([buf[o], buf[o + 1]]);
        Some(Self {
            accel: [rd(0), rd(2), rd(4)],
            gyro: [rd(6), rd(8), rd(10)],
        })
    }
}

pub struct ThreeDsDevice {
    metadata: DeviceMetadata,
    packet_rx: Option<mpsc::Receiver<ThreeDsPacket>>,
    reader: Option<JoinHandle<()>>,
}

impl ThreeDsDevice {
    pub fn new(metadata: DeviceMetadata, packet_rx: mpsc::Receiver<ThreeDsPacket>) -> Self {
        Self {
            metadata,
            packet_rx: Some(packet_rx),
            reader: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for ThreeDsDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let mut packet_rx = self
            .packet_rx
            .take()
            .ok_or_else(|| DeviceError::Hid("3ds already started".into()))?;
        let (tx, rx) = mpsc::channel::<ChannelInfo>(256);
        let id = self.metadata.id.clone();
        self.reader = Some(tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id.clone())).await;
            let start = Instant::now();
            // Gravity auto-scale: the 3DS accel LSB/g is unpublished and varies
            // per console revision, so derive the m/s²-per-raw factor from the
            // resting gravity magnitude over the first N samples instead of
            // hardcoding a constant. Mirrors the known-working forwarder.
            let mut grav_accum: i64 = 0;
            let mut grav_count: u32 = 0;
            let mut division: f32 = 0.0;
            while let Some(pkt) = packet_rx.recv().await {
                if grav_count < GRAVITY_SAMPLES {
                    grav_accum += (pkt.accel[1] as i64).abs();
                    grav_count += 1;
                    let mean = (grav_accum as f32 / grav_count as f32).max(1.0);
                    division = G / mean;
                    // Hold output until the scale settles to avoid a startup
                    // spike of wrongly-scaled accel reaching fusion.
                    continue;
                }
                let imu = imu_from_raw(pkt, division, start, Instant::now());
                if tx.send(ChannelInfo::ImuSamples(vec![imu])).await.is_err() {
                    break;
                }
            }
            let _ = tx.send(ChannelInfo::Disconnected).await;
        }));
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(h) = self.reader.take() {
            h.abort();
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        // No LED in the forwarded packet.
        Ok(())
    }

    async fn set_rumble(&mut self, _intensity: f32) -> Result<(), DeviceError> {
        // No rumble back-channel.
        Ok(())
    }
}

const G: f32 = 9.80665;
const GRAVITY_SAMPLES: u32 = 100;
/// Raw gyro LSB → rad/s. Recovered from the known-working forwarder; consistent
/// with the 3DS InvenSense part (≈ 0.0716 °/s per LSB). Validate per hardware.
const GYRO_RAD_PER_LSB: f32 = 0.00125;

/// Map a raw packet into the pipeline `ImuSample` (m/s² accel, rad/s gyro).
///
/// ⚠ Axis convention is provisional — ported from the known-working forwarder
/// (accel `(ax, az, ay)`, gyro `(-gx, -gy, -gz)`). It must be confirmed on a
/// live console (gravity = +Z screen-up, gyro sign agrees with accel-derived
/// rotation) before it is treated as canonical. See `docs/reference/3ds_protocol.md`.
fn imu_from_raw(pkt: ThreeDsPacket, division: f32, start: Instant, now: Instant) -> ImuSample {
    let a = pkt.accel;
    let g = pkt.gyro;
    ImuSample {
        accel: [
            a[0] as f32 * division,
            a[2] as f32 * division,
            a[1] as f32 * division,
        ],
        gyro: [
            -(g[0] as f32) * GYRO_RAD_PER_LSB,
            -(g[1] as f32) * GYRO_RAD_PER_LSB,
            -(g[2] as f32) * GYRO_RAD_PER_LSB,
        ],
        mag: None,
        timestamp_us: now.duration_since(start).as_micros() as u64,
    }
}

pub fn metadata_for_key(key: &str) -> DeviceMetadata {
    DeviceMetadata {
        id: device_traits::DeviceId {
            mac: stable_mac(key.as_bytes()),
            serial: key.to_string(),
        },
        kind: device_traits::DeviceKind::ThreeDs,
        firmware: Some("forwarded-homebrew".into()),
        capabilities: DeviceCapabilities {
            has_magnetometer: false,
            has_battery: false,
            has_rumble: false,
            native_imu_rate_hz: 100,
        },
    }
}

/// FNV-1a hash of the routing key → a stable, locally-administered MAC so
/// SlimeVR-Server keeps one tracker identity per console across reconnects.
fn stable_mac(bytes: &[u8]) -> [u8; 6] {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&hash.to_le_bytes()[0..6]);
    mac[0] = (mac[0] & 0xFE) | 0x02;
    mac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_wrong_length() {
        assert!(ThreeDsPacket::parse(&[0u8; 11]).is_none());
        assert!(ThreeDsPacket::parse(&[0u8; 13]).is_none());
    }

    #[test]
    fn parse_decodes_little_endian_6axis() {
        // ax=1, ay=2, az=3, gx=-1, gy=256, gz=-2
        let raw = [
            0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0xFF, 0xFF, 0x00, 0x01, 0xFE, 0xFF,
        ];
        let p = ThreeDsPacket::parse(&raw).expect("parse");
        assert_eq!(p.accel, [1, 2, 3]);
        assert_eq!(p.gyro, [-1, 256, -2]);
    }

    #[test]
    fn gravity_autoscale_normalizes_resting_axis() {
        // ay rests on gravity at raw 400 → division = 9.80665/400.
        let division = G / 400.0;
        let p = ThreeDsPacket {
            accel: [0, 400, 0],
            gyro: [0, 0, 0],
        };
        let s = imu_from_raw(p, division, Instant::now(), Instant::now());
        // ay maps to output Z.
        assert!(
            (s.accel[2] - G).abs() < 0.01,
            "z should be ~1g, got {}",
            s.accel[2]
        );
        assert!(s.accel[0].abs() < 0.01);
        assert!(s.accel[1].abs() < 0.01);
    }

    #[test]
    fn gyro_scaled_and_negated() {
        let p = ThreeDsPacket {
            accel: [0, 1, 0],
            gyro: [1000, -1000, 0],
        };
        let s = imu_from_raw(p, 1.0, Instant::now(), Instant::now());
        assert!((s.gyro[0] + 1.25).abs() < 1e-4, "got {}", s.gyro[0]);
        assert!((s.gyro[1] - 1.25).abs() < 1e-4, "got {}", s.gyro[1]);
    }

    #[test]
    fn stable_mac_is_locally_administered() {
        let m = stable_mac(b"192.168.1.50");
        assert_eq!(m[0] & 0x01, 0);
        assert_eq!(m[0] & 0x02, 0x02);
    }
}

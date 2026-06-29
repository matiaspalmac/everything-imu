use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceMetadata, ImuSample,
};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// One decoded 24-byte UDP packet: full 6-axis IMU as little-endian `f32`.
///
/// `sceMotionGetSensorState` returns the accelerometer in g and the gyroscope
/// in rad/s, so the homebrew forwards those floats verbatim — no raw counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VitaPacket {
    /// Accelerometer X/Y/Z in g.
    pub accel_g: [f32; 3],
    /// Gyroscope X/Y/Z in rad/s.
    pub gyro_rad: [f32; 3],
}

impl VitaPacket {
    pub const WIRE_LEN: usize = 24;

    /// Parse a wire packet. Returns `None` unless exactly 24 bytes.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() != Self::WIRE_LEN {
            return None;
        }
        let rd = |o: usize| f32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
        let accel_g = [rd(0), rd(4), rd(8)];
        let gyro_rad = [rd(12), rd(16), rd(20)];
        // Reject NaN/Inf so garbage datagrams cannot poison downstream fusion.
        if !accel_g.iter().chain(gyro_rad.iter()).all(|v| v.is_finite()) {
            return None;
        }
        Some(Self { accel_g, gyro_rad })
    }
}

pub struct VitaDevice {
    metadata: DeviceMetadata,
    packet_rx: Option<mpsc::Receiver<VitaPacket>>,
    reader: Option<JoinHandle<()>>,
}

impl VitaDevice {
    pub fn new(metadata: DeviceMetadata, packet_rx: mpsc::Receiver<VitaPacket>) -> Self {
        Self {
            metadata,
            packet_rx: Some(packet_rx),
            reader: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for VitaDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let mut packet_rx = self
            .packet_rx
            .take()
            .ok_or_else(|| DeviceError::Hid("vita already started".into()))?;
        let (tx, rx) = mpsc::channel::<ChannelInfo>(256);
        let id = self.metadata.id.clone();
        self.reader = Some(tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id.clone())).await;
            let start = Instant::now();
            while let Some(pkt) = packet_rx.recv().await {
                let imu = imu_from_packet(pkt, start, Instant::now());
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
        Ok(())
    }

    async fn set_rumble(&mut self, _intensity: f32) -> Result<(), DeviceError> {
        Ok(())
    }
}

const G: f32 = 9.80665;

/// Map a packet into the pipeline `ImuSample` (m/s² accel, rad/s gyro).
///
/// ⚠ Axis convention is provisional — passed through as `sceMotion` reports it.
/// Confirm on a live Vita (gravity = +Z screen-up, gyro sign agrees with
/// accel-derived rotation) before treating as canonical. See
/// `docs/reference/vita_protocol.md`.
fn imu_from_packet(pkt: VitaPacket, start: Instant, now: Instant) -> ImuSample {
    ImuSample {
        accel: [pkt.accel_g[0] * G, pkt.accel_g[1] * G, pkt.accel_g[2] * G],
        gyro: pkt.gyro_rad,
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
        kind: device_traits::DeviceKind::Vita,
        firmware: Some("forwarded-homebrew".into()),
        capabilities: DeviceCapabilities {
            has_magnetometer: false,
            has_battery: false,
            has_rumble: false,
            native_imu_rate_hz: 100,
        },
    }
}

/// FNV-1a hash of the routing key → a stable, locally-administered MAC.
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
        assert!(VitaPacket::parse(&[0u8; 23]).is_none());
        assert!(VitaPacket::parse(&[0u8; 25]).is_none());
    }

    #[test]
    fn parse_decodes_little_endian_floats() {
        let mut raw = [0u8; 24];
        raw[0..4].copy_from_slice(&1.0f32.to_le_bytes());
        raw[4..8].copy_from_slice(&(-1.0f32).to_le_bytes());
        raw[8..12].copy_from_slice(&0.5f32.to_le_bytes());
        raw[12..16].copy_from_slice(&2.0f32.to_le_bytes());
        raw[16..20].copy_from_slice(&(-3.0f32).to_le_bytes());
        raw[20..24].copy_from_slice(&0.25f32.to_le_bytes());
        let p = VitaPacket::parse(&raw).expect("parse");
        assert_eq!(p.accel_g, [1.0, -1.0, 0.5]);
        assert_eq!(p.gyro_rad, [2.0, -3.0, 0.25]);
    }

    #[test]
    fn accel_g_converted_to_m_s2() {
        let p = VitaPacket {
            accel_g: [0.0, 0.0, 1.0],
            gyro_rad: [0.0, 0.0, 0.0],
        };
        let s = imu_from_packet(p, Instant::now(), Instant::now());
        assert!((s.accel[2] - G).abs() < 1e-4, "got {}", s.accel[2]);
    }

    #[test]
    fn gyro_passed_through_in_rad_s() {
        let p = VitaPacket {
            accel_g: [0.0, 0.0, 1.0],
            gyro_rad: [0.1, -0.2, 0.3],
        };
        let s = imu_from_packet(p, Instant::now(), Instant::now());
        assert_eq!(s.gyro, [0.1, -0.2, 0.3]);
    }

    #[test]
    fn stable_mac_is_locally_administered() {
        let m = stable_mac(b"192.168.1.77");
        assert_eq!(m[0] & 0x01, 0);
        assert_eq!(m[0] & 0x02, 0x02);
    }
}

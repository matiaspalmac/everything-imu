use device_traits::{
    BatteryState, ButtonState, ChannelInfo, Device, DeviceCapabilities, DeviceError,
    DeviceMetadata, ResetButtonDetector,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Copy)]
pub struct WiiPacket {
    pub accel: [i16; 3],
    pub data: [i16; 3],
    pub nunchuk_connected: bool,
    pub battery_level: u8,
    pub button_up: bool,
}

pub struct WiiDevice {
    metadata: DeviceMetadata,
    packet_rx: Option<mpsc::Receiver<WiiPacket>>,
    stream_key: String,
    rumble_state: Arc<RwLock<std::collections::HashMap<String, [u8; 4]>>>,
    reader: Option<JoinHandle<()>>,
}

impl WiiDevice {
    pub fn new(
        metadata: DeviceMetadata,
        packet_rx: mpsc::Receiver<WiiPacket>,
        stream_key: String,
        rumble_state: Arc<RwLock<std::collections::HashMap<String, [u8; 4]>>>,
    ) -> Self {
        Self {
            metadata,
            packet_rx: Some(packet_rx),
            stream_key,
            rumble_state,
            reader: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for WiiDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let mut packet_rx = self
            .packet_rx
            .take()
            .ok_or_else(|| DeviceError::Hid("wii already started".into()))?;
        let (tx, rx) = mpsc::channel::<ChannelInfo>(256);
        let id = self.metadata.id.clone();
        self.reader = Some(tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id.clone())).await;
            let mut reset = ResetButtonDetector::default();
            let start = Instant::now();
            let mut last_battery = Instant::now()
                .checked_sub(Duration::from_secs(2))
                .unwrap_or_else(Instant::now);
            let mut last_battery_level: Option<u8> = None;
            while let Some(pkt) = packet_rx.recv().await {
                let now = Instant::now();
                // Nunchuk path forwards only accel because the companion sends
                // accel via `pkt.accel` and MotionPlus-style gyro arrives in
                // `pkt.data` only when no nunchuk is attached.
                let imu = if pkt.nunchuk_connected {
                    imu_from_raw(pkt.accel, [0, 0, 0], start, now)
                } else {
                    let gyro = [
                        pkt.data[0].saturating_sub(8192),
                        pkt.data[1].saturating_sub(8192),
                        pkt.data[2].saturating_sub(8192),
                    ];
                    imu_from_raw(pkt.accel, gyro, start, now)
                };
                if tx.send(ChannelInfo::ImuSamples(vec![imu])).await.is_err() {
                    break;
                }
                // Throttle battery to 1 Hz or on-change. Packet rate ~100 Hz.
                let changed = last_battery_level != Some(pkt.battery_level);
                if changed || now.duration_since(last_battery) >= Duration::from_secs(1) {
                    last_battery = now;
                    last_battery_level = Some(pkt.battery_level);
                    let _ = tx
                        .send(ChannelInfo::Battery(BatteryState {
                            fraction: (pkt.battery_level as f32 / 100.0).clamp(0.0, 1.0),
                            charging: false,
                        }))
                        .await;
                }
                if let Some(kind) = reset.observe(
                    ButtonState::CaptureOnly {
                        pressed: pkt.button_up,
                    },
                    now,
                ) {
                    let _ = tx.send(ChannelInfo::ResetRequested(kind)).await;
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

    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError> {
        // Wiimote has a 1-bit motor — engage above the halfway mark.
        let on = device_traits::rumble::is_on(intensity, 0.5);
        let mut state = self.rumble_state.write().await;
        if let Some((base_ip, idx)) = self.stream_key.rsplit_once(':') {
            if let Some(slot) = state.get_mut(base_ip) {
                if let Ok(i) = idx.parse::<usize>() {
                    if i < 4 {
                        slot[i] = if on { 1 } else { 0 };
                    }
                }
            }
        }
        Ok(())
    }
}

fn imu_from_raw(
    accel_raw: [i16; 3],
    gyro_raw: [i16; 3],
    start: Instant,
    now: Instant,
) -> device_traits::ImuSample {
    const G: f32 = 9.80665;
    // Wiimote core accelerometer: ~512 raw at 0 g, ~200 LSB/g (ADXL330-class).
    // Matches the legacy `(raw-512)/200*g` mapping — zero-offset matters, the
    // earlier `raw/512` form put rest gravity at the wrong magnitude and never
    // settled to ‖a‖≈9.81 m/s². See `docs/reference/wii_protocol.md`.
    const ACCEL_ZERO_G: f32 = 512.0;
    const ACCEL_LSB_PER_G: f32 = 200.0;
    const GYRO_DPS_PER_LSB: f32 = 0.07;
    const DEG_TO_RAD: f32 = core::f32::consts::PI / 180.0;

    let accel_g = |raw: i16| (raw as f32 - ACCEL_ZERO_G) / ACCEL_LSB_PER_G * G;
    device_traits::ImuSample {
        gyro: [
            gyro_raw[0] as f32 * GYRO_DPS_PER_LSB * DEG_TO_RAD,
            gyro_raw[1] as f32 * GYRO_DPS_PER_LSB * DEG_TO_RAD,
            gyro_raw[2] as f32 * GYRO_DPS_PER_LSB * DEG_TO_RAD,
        ],
        accel: [
            accel_g(accel_raw[0]),
            accel_g(accel_raw[1]),
            accel_g(accel_raw[2]),
        ],
        mag: None,
        timestamp_us: now.duration_since(start).as_micros() as u64,
    }
}

pub fn metadata_for_key(key: &str) -> DeviceMetadata {
    let mac = stable_mac(key.as_bytes());
    DeviceMetadata {
        id: device_traits::DeviceId {
            mac,
            serial: key.to_string(),
        },
        kind: device_traits::DeviceKind::Wii,
        firmware: Some("forwarded-companion".into()),
        capabilities: DeviceCapabilities {
            has_magnetometer: false,
            has_battery: true,
            has_rumble: true,
            native_imu_rate_hz: 100,
        },
    }
}

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
    fn stable_mac_is_locally_administered() {
        let m = stable_mac(b"127.0.0.1:0");
        assert_eq!(m[0] & 0x01, 0);
        assert_eq!(m[0] & 0x02, 0x02);
    }

    #[test]
    fn accel_scale_zero_offset_gives_one_g_at_rest() {
        // At rest, lying flat: X,Y at the 512 zero point, Z one g above it.
        let start = Instant::now();
        let s = imu_from_raw([512, 512, 712], [0, 0, 0], start, start);
        assert!(
            s.accel[0].abs() < 0.01,
            "x should be ~0, got {}",
            s.accel[0]
        );
        assert!(
            s.accel[1].abs() < 0.01,
            "y should be ~0, got {}",
            s.accel[1]
        );
        assert!(
            (s.accel[2] - 9.80665).abs() < 0.05,
            "z should be ~1g, got {}",
            s.accel[2]
        );
    }

    #[test]
    fn accel_negative_offset_below_zero_point() {
        let start = Instant::now();
        let s = imu_from_raw([312, 512, 512], [0, 0, 0], start, start);
        // (312-512)/200 = -1 g.
        assert!((s.accel[0] + 9.80665).abs() < 0.05, "got {}", s.accel[0]);
    }
}

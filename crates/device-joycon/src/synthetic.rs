//! Deterministic synthetic Joy-Con — feature `synthetic-source`.

use device_traits::{
    BatteryState, ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind,
    DeviceMetadata, ImuSample,
};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct SyntheticJoyConL {
    metadata: DeviceMetadata,
    seed: u64,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl SyntheticJoyConL {
    pub fn new(seed: u64) -> Self {
        let mac = derive_mac(seed);
        let id = DeviceId {
            mac,
            serial: format!("synthetic-{seed:04x}"),
        };
        Self {
            metadata: DeviceMetadata {
                id,
                kind: DeviceKind::JoyConL,
                firmware: Some("synthetic 0.1".into()),
                capabilities: DeviceCapabilities {
                    has_magnetometer: false,
                    has_battery: true,
                    has_rumble: false,
                    native_imu_rate_hz: 200,
                },
            },
            seed,
            handle: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for SyntheticJoyConL {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let (tx, rx) = mpsc::channel(64);
        let id = self.metadata.id.clone();
        let _seed = self.seed;
        let h = tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id.clone())).await;
            let mut interval = tokio::time::interval(Duration::from_millis(15));
            let mut t = 0.0_f32;
            let mut packet_idx = 0_u32;
            loop {
                interval.tick().await;
                let mut samples = Vec::with_capacity(3);
                for sample_in_packet in 0..3i32 {
                    let gz = (t * 0.5).sin() * 0.5;
                    samples.push(ImuSample {
                        gyro: [0.0, 0.0, gz],
                        accel: [0.0, 0.0, 9.806_65],
                        mag: None,
                        timestamp_us: ((t - (2 - sample_in_packet) as f32 * 0.005) * 1e6) as u64,
                    });
                    t += 0.005;
                }
                if tx.send(ChannelInfo::ImuSamples(samples)).await.is_err() {
                    break;
                }
                packet_idx = packet_idx.wrapping_add(1);
                if packet_idx % 60 == 0 {
                    let _ = tx
                        .send(ChannelInfo::Battery(BatteryState {
                            fraction: 0.75,
                            charging: false,
                        }))
                        .await;
                }
            }
        });
        self.handle = Some(h);
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(h) = self.handle.take() {
            h.abort();
        }
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn set_rumble(&mut self, _on: bool) -> Result<(), DeviceError> {
        Ok(())
    }
}

fn derive_mac(seed: u64) -> [u8; 6] {
    let bytes = seed.to_le_bytes();
    [0x02, 0x57, 0x45, bytes[0], bytes[1], bytes[2]]
}

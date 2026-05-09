//! Deterministic synthetic PS Move — feature `synthetic-source`.

use device_traits::{
    BatteryState, ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind,
    DeviceMetadata, ImuSample,
};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct SyntheticPsMove {
    metadata: DeviceMetadata,
    seed: u64,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl SyntheticPsMove {
    pub fn new(seed: u64) -> Self {
        let mac = derive_mac(seed);
        let id = DeviceId {
            mac,
            serial: format!("synth-move-{seed:04x}"),
        };
        Self {
            metadata: DeviceMetadata {
                id,
                kind: DeviceKind::PsMoveZcm1,
                firmware: Some("synthetic 0.1".into()),
                capabilities: DeviceCapabilities {
                    has_magnetometer: true,
                    has_battery: true,
                    has_rumble: true,
                    native_imu_rate_hz: 175,
                },
            },
            seed,
            handle: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for SyntheticPsMove {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let (tx, rx) = mpsc::channel(64);
        let id = self.metadata.id.clone();
        let _seed = self.seed;
        let h = tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id.clone())).await;
            // 175 Hz native; emit pairs of samples per 11 ms tick to match
            // the real ZCM1 sub-rate doubling pattern.
            let mut interval = tokio::time::interval(Duration::from_millis(11));
            let mut t = 0.0_f32;
            let mut packet_idx = 0_u32;
            loop {
                interval.tick().await;
                let mut samples = Vec::with_capacity(2);
                for sub in 0..2 {
                    let gz = (t * 0.4).sin() * 0.6;
                    samples.push(ImuSample {
                        gyro: [0.0, 0.0, gz],
                        accel: [0.0, 0.0, 9.806_65],
                        mag: Some([10.0, 20.0, 30.0]),
                        timestamp_us: ((t - (1 - sub) as f32 * 0.0055) * 1e6) as u64,
                    });
                    t += 0.0055;
                }
                if tx.send(ChannelInfo::ImuSamples(samples)).await.is_err() {
                    break;
                }
                packet_idx = packet_idx.wrapping_add(1);
                if packet_idx % 90 == 0 {
                    let _ = tx
                        .send(ChannelInfo::Battery(BatteryState {
                            fraction: 0.55,
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
    [0x02, 0x4D, 0x56, bytes[0], bytes[1], bytes[2]]
}

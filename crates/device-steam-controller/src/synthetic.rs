//! Synthetic Steam Controller — feature `synthetic-source`.

use device_traits::{
    ChannelInfo, Device, DeviceCapabilities, DeviceError, DeviceId, DeviceKind, DeviceMetadata,
    ImuSample,
};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct SyntheticSteamController {
    metadata: DeviceMetadata,
    seed: u64,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl SyntheticSteamController {
    pub fn new(seed: u64) -> Self {
        let mac = derive_mac(seed);
        let id = DeviceId {
            mac,
            serial: format!("synth-steamctrl-{seed:04x}"),
        };
        Self {
            metadata: DeviceMetadata {
                id,
                kind: DeviceKind::SteamController,
                firmware: Some("synthetic 0.1".into()),
                capabilities: DeviceCapabilities {
                    has_magnetometer: false,
                    has_battery: true,
                    has_rumble: true,
                    native_imu_rate_hz: 100,
                },
            },
            seed,
            handle: None,
        }
    }
}

#[async_trait::async_trait]
impl Device for SyntheticSteamController {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let (tx, rx) = mpsc::channel(64);
        let id = self.metadata.id.clone();
        let _seed = self.seed;
        let h = tokio::spawn(async move {
            let _ = tx.send(ChannelInfo::Connected(id)).await;
            let mut interval = tokio::time::interval(Duration::from_millis(10));
            let mut t = 0.0_f32;
            loop {
                interval.tick().await;
                let gz = (t * 0.5).sin() * 0.5;
                let sample = ImuSample {
                    gyro: [0.0, 0.0, gz],
                    accel: [0.0, 0.0, 9.806_65],
                    mag: None,
                    timestamp_us: (t * 1e6) as u64,
                };
                if tx
                    .send(ChannelInfo::ImuSamples(vec![sample]))
                    .await
                    .is_err()
                {
                    break;
                }
                t += 0.010;
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

    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError> {
        tracing::debug!(intensity, "synthetic steam ctrl rumble");
        Ok(())
    }
}

fn derive_mac(seed: u64) -> [u8; 6] {
    let bytes = seed.to_le_bytes();
    [0x02, 0x28, 0xDE, 0x11, bytes[0], bytes[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn synthetic_emits_connected_then_samples() {
        let mut dev = SyntheticSteamController::new(0xBEEF);
        let mut rx = dev.start().await.unwrap();
        let ev = rx.recv().await.unwrap();
        assert!(matches!(ev, ChannelInfo::Connected(_)));
        tokio::time::advance(Duration::from_millis(50)).await;
        let next = rx.recv().await.unwrap();
        assert!(matches!(next, ChannelInfo::ImuSamples(_)));
        dev.stop().await.unwrap();
    }
}

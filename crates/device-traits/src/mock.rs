//! Generic mock Device for downstream test crates (`core`, `device-joycon`).
//!
//! Behind the `mock` feature so it doesn't bloat release builds.

use crate::device::{Device, DeviceCapabilities, DeviceError, DeviceKind, DeviceMetadata};
use crate::events::ChannelInfo;
use crate::DeviceId;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub struct MockDevice {
    pub metadata: DeviceMetadata,
    /// Pre-built event sequence — emitted in order on `start()`.
    pub script: Arc<Mutex<Vec<ChannelInfo>>>,
}

impl MockDevice {
    pub fn new(serial: &str, kind: DeviceKind) -> Self {
        let id = DeviceId {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            serial: serial.into(),
        };
        Self {
            metadata: DeviceMetadata {
                id,
                kind,
                firmware: Some("mock 0.1".into()),
                capabilities: DeviceCapabilities {
                    has_magnetometer: false,
                    has_battery: true,
                    has_rumble: false,
                    native_imu_rate_hz: 200,
                },
            },
            script: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Append events that will be emitted on `start()`. Useful in tests.
    pub async fn push(&self, ev: ChannelInfo) {
        self.script.lock().await.push(ev);
    }
}

#[async_trait::async_trait]
impl Device for MockDevice {
    fn metadata(&self) -> &DeviceMetadata {
        &self.metadata
    }

    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError> {
        let (tx, rx) = mpsc::channel(64);
        let script = self.script.clone();
        tokio::spawn(async move {
            let evs: Vec<ChannelInfo> = std::mem::take(&mut *script.lock().await);
            for e in evs {
                if tx.send(e).await.is_err() {
                    return;
                }
            }
        });
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn set_rumble(&mut self, _on: bool) -> Result<(), DeviceError> {
        Ok(())
    }
}

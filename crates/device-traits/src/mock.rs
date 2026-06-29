//! Generic mock Device for downstream test crates (`core`, `device-joycon`).
//!
//! Behind the `mock` feature so it doesn't bloat release builds.

use crate::device::{Device, DeviceCapabilities, DeviceError, DeviceKind, DeviceMetadata};
use crate::events::ChannelInfo;
use crate::DeviceId;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

/// Monotonic counter feeding [`MockDevice::new`] so each mock has a unique
/// locally-administered MAC. Without this, two mocks in the same process
/// collide on `DeviceId.mac` and `AppState`'s mac-keyed lookups return the
/// wrong device.
static MOCK_MAC_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct MockDevice {
    pub metadata: DeviceMetadata,
    /// Pre-built event sequence — emitted in order on `start()`.
    pub script: Arc<Mutex<Vec<ChannelInfo>>>,
    /// Handle for the emitter task, populated by `start` and joined by `stop`.
    task: Option<JoinHandle<()>>,
    /// Set in `start`, cleared in `stop`. Tracked independently of the emitter
    /// task so a restart after the script drains still errors per contract.
    started: bool,
}

impl MockDevice {
    pub fn new(serial: &str, kind: DeviceKind) -> Self {
        let n = MOCK_MAC_COUNTER.fetch_add(1, Ordering::Relaxed);
        // Locally-administered, unicast MAC: low bit of first byte = 0, second
        // bit = 1. Encodes the counter across the remaining 5 bytes so up to
        // 2^40 mocks per process are unique.
        let mac = [
            0x02,
            ((n >> 32) & 0xFF) as u8,
            ((n >> 24) & 0xFF) as u8,
            ((n >> 16) & 0xFF) as u8,
            ((n >> 8) & 0xFF) as u8,
            (n & 0xFF) as u8,
        ];
        let id = DeviceId {
            mac,
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
            task: None,
            started: false,
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
        // Honour the documented `Device::start` contract: a second start
        // without an intervening stop must error rather than silently
        // double-spawning the emitter.
        if self.started {
            return Err(DeviceError::Hid("MockDevice already started".into()));
        }
        let (tx, rx) = mpsc::channel(64);
        let script = self.script.clone();
        let handle = tokio::spawn(async move {
            let evs: Vec<ChannelInfo> = std::mem::take(&mut *script.lock().await);
            for e in evs {
                if tx.send(e).await.is_err() {
                    return;
                }
            }
        });
        self.task = Some(handle);
        self.started = true;
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), DeviceError> {
        if let Some(h) = self.task.take() {
            h.abort();
            let _ = h.await;
        }
        self.started = false;
        Ok(())
    }

    async fn set_led_mask(&mut self, _mask: u8) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn set_rumble(&mut self, _intensity: f32) -> Result<(), DeviceError> {
        Ok(())
    }
}

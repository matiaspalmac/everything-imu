//! `TeslaFactory` — single-instance `DeviceFactory` impl.
//!
//! Unlike HID device factories that scan for hotplug events, the Tesla
//! bridge has exactly one configured vehicle at a time. We emit a single
//! `(metadata, device)` pair when the supervisor starts the factory and
//! then exit the enumerate loop.

use device_traits::{Device, DeviceError, DeviceFactory, DeviceMetadata};
use tokio::sync::mpsc;

use crate::config::TeslaConfig;
use crate::device::TeslaDevice;

pub struct TeslaFactory {
    config: TeslaConfig,
}

impl TeslaFactory {
    pub fn new(config: TeslaConfig) -> Self {
        Self { config }
    }

    pub fn synthetic() -> Self {
        Self {
            config: TeslaConfig::Synthetic(Default::default()),
        }
    }
}

#[async_trait::async_trait]
impl DeviceFactory for TeslaFactory {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError> {
        let device = TeslaDevice::new(self.config.clone());
        let meta = device.metadata().clone();
        // Best-effort send: if the receiver is gone the supervisor shut down,
        // which is not a Tesla-side error.
        if out.send((meta, Box::new(device))).await.is_err() {
            return Ok(());
        }
        // Hold the sender open. The supervisor exits its register loop when
        // every factory's enumerate channel closes, so returning here would
        // tear the whole pipeline down on configurations that only run the
        // Tesla bridge. Keep blocking until the supervisor drops the
        // receiver (which propagates back as a send error on the next loop
        // iteration; we never actually send again).
        out.closed().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn factory_emits_one_device_and_holds_channel_open() {
        let (tx, mut rx) = mpsc::channel(4);
        let factory = TeslaFactory::synthetic();
        // enumerate_loop blocks on out.closed(); race it against draining
        // the device so we can verify the emit without deadlocking.
        let handle = tokio::spawn(async move {
            let _ = factory.enumerate_loop(tx).await;
        });
        let first = rx.recv().await.expect("device emitted");
        assert_eq!(first.0.kind, device_traits::DeviceKind::Tesla);
        // Drop the receiver to unblock enumerate_loop.
        drop(rx);
        handle.await.expect("factory join");
    }
}

//! Supervisor — runs DeviceFactory enumerate loops and registers discovered devices.

use crate::app_state::{AppState, DeviceControl};
use crate::error::AppError;
use device_traits::{Device, DeviceFactory, DeviceMetadata};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct Supervisor {
    state: Arc<AppState>,
    factories: Vec<Arc<dyn DeviceFactory>>,
}

impl Supervisor {
    pub fn new(state: Arc<AppState>, factories: Vec<Arc<dyn DeviceFactory>>) -> Self {
        Self { state, factories }
    }

    pub async fn run(self) -> Result<(), AppError> {
        let (tx, mut rx) = mpsc::channel::<(DeviceMetadata, Box<dyn Device>)>(16);

        for f in &self.factories {
            let factory = f.clone();
            let txc = tx.clone();
            tokio::spawn(async move {
                if let Err(e) = factory.enumerate_loop(txc).await {
                    tracing::warn!(error = %e, "factory enumerate_loop exited");
                }
            });
        }
        drop(tx);

        while let Some((meta, mut device)) = rx.recv().await {
            tracing::info!(id = %meta.id, kind = ?meta.kind, "device discovered");
            let events = match device.start().await {
                Ok(rx) => rx,
                Err(e) => {
                    tracing::warn!(id = %meta.id, error = %e, "device start failed");
                    continue;
                }
            };
            let (control_tx, mut control_rx) = mpsc::channel::<DeviceControl>(16);
            let device_id = meta.id.clone();
            tokio::spawn(async move {
                while let Some(cmd) = control_rx.recv().await {
                    let res = match cmd {
                        DeviceControl::SetLedMask(mask) => device.set_led_mask(mask).await,
                        DeviceControl::SetRumble(on) => device.set_rumble(on).await,
                    };
                    if let Err(e) = res {
                        tracing::debug!(id = %device_id, error = %e, "device control command failed");
                    }
                }
                let _ = device.stop().await;
            });
            if let Err(e) = self.state.register_device(meta, events, control_tx).await {
                tracing::warn!(error = %e, "register_device failed");
            }
        }
        Ok(())
    }
}

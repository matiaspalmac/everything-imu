//! Supervisor — runs DeviceFactory enumerate loops and registers discovered devices.

use crate::app_state::AppState;
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
            if let Err(e) = self.state.register_device(meta, events).await {
                tracing::warn!(error = %e, "register_device failed");
            }
        }
        Ok(())
    }
}

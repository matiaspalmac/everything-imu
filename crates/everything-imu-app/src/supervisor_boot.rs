//! Boot the Supervisor with a chosen factory selection.

use crate::dto::DeviceMetadataDto;
use crate::events::DeviceDiscovered;
use crate::state::AppHandle;
use device_dualsense::DualSenseFactory;
use device_joycon::JoyconFactory;
use device_psmove::PsMoveFactory;
use device_traits::DeviceFactory;
use device_wii::WiiFactory;
use everything_imu_core::Supervisor;
use std::sync::Arc;
use tauri::{AppHandle as TauriAppHandle, Manager};
use tauri_specta::Event;

pub fn spawn(app: &TauriAppHandle, auto_start_synthetic: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let handle = match app.try_state::<AppHandle>() {
            Some(h) => h,
            None => return,
        };

        // Forward every device registration to the UI. Subscribe before the
        // supervisor starts so no device registered during boot is missed.
        {
            let mut rx = handle.state.subscribe_device_events();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(meta) => {
                            let _ = DeviceDiscovered {
                                metadata: DeviceMetadataDto::from(&meta),
                            }
                            .emit(&app);
                        }
                        // Lagged: a burst dropped some events. The UI still
                        // has list_devices for reconciliation — keep going.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }
        let factories: Vec<Arc<dyn DeviceFactory>> = if auto_start_synthetic {
            // Spawn one of each kind so the UI can be exercised against a
            // mixed device pool without paired hardware.
            vec![
                Arc::new(JoyconFactory::synthetic(1)),
                Arc::new(DualSenseFactory::synthetic(1)),
                Arc::new(PsMoveFactory::synthetic(1)),
            ]
        } else {
            vec![
                Arc::new(JoyconFactory::real()),
                Arc::new(DualSenseFactory::new()),
                Arc::new(PsMoveFactory::new()),
                Arc::new(WiiFactory::new()),
            ]
        };
        let sup = Supervisor::new(handle.state.clone(), factories);
        if let Err(e) = sup.run().await {
            tracing::warn!(error = %e, "supervisor exited");
        }
    });
}

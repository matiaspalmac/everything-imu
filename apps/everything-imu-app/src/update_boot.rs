//! Boot-time auto-update task.
//!
//! Spawned once at app setup. Reads the `auto_update_on_startup` setting
//! (default true), waits a few seconds so the UI has time to mount its
//! event listener, then checks GitHub releases. When a newer build
//! exists it emits a sequence of `UpdateStatus` events — the UI surfaces
//! these as a toast/banner. Install is automatic by default but the
//! user can flip `auto_install_on_startup` to false to keep this
//! check-only.
//!
//! All failures are non-fatal — the app continues regardless.

use crate::events::{UpdateStage, UpdateStatus};
use crate::state::AppHandle;
use crate::updater;
use std::time::Duration;
use tauri::{AppHandle as TauriAppHandle, Manager};
use tauri_specta::Event;

const STARTUP_DELAY: Duration = Duration::from_secs(4);

pub fn spawn(app: &TauriAppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;

        let handle = match app.try_state::<AppHandle>() {
            Some(h) => h,
            None => return,
        };

        // Two opt-outs: one to skip the check entirely, one to skip the
        // automatic install. Defaults are check=on, install=on; users who
        // dislike surprise restarts can flip the install bit off and
        // still get the toast.
        let check_enabled = handle
            .db
            .get_setting("auto_update_on_startup")
            .ok()
            .flatten()
            .map(|v| v != "0")
            .unwrap_or(true);
        if !check_enabled {
            return;
        }
        let install_enabled = handle
            .db
            .get_setting("auto_install_on_startup")
            .ok()
            .flatten()
            .map(|v| v != "0")
            .unwrap_or(true);

        let _ = UpdateStatus {
            stage: UpdateStage::Checking,
        }
        .emit(&app);

        let info = match updater::check(app.clone()).await {
            Ok(i) => i,
            Err(e) => {
                let _ = UpdateStatus {
                    stage: UpdateStage::Failed {
                        message: e.to_string(),
                    },
                }
                .emit(&app);
                return;
            }
        };

        if !info.update_available {
            let _ = UpdateStatus {
                stage: UpdateStage::NoUpdate {
                    current: info.current,
                },
            }
            .emit(&app);
            return;
        }

        let _ = UpdateStatus {
            stage: UpdateStage::Available {
                current: info.current.clone(),
                latest: info.latest.clone(),
            },
        }
        .emit(&app);

        if !install_enabled {
            return;
        }

        let _ = UpdateStatus {
            stage: UpdateStage::Installing {
                current: info.current.clone(),
                latest: info.latest.clone(),
            },
        }
        .emit(&app);

        match updater::apply(app.clone()).await {
            Ok(applied) => {
                let _ = UpdateStatus {
                    stage: UpdateStage::Installed {
                        latest: applied.latest,
                    },
                }
                .emit(&app);
            }
            Err(e) => {
                let _ = UpdateStatus {
                    stage: UpdateStage::Failed {
                        message: e.to_string(),
                    },
                }
                .emit(&app);
            }
        }
    });
}

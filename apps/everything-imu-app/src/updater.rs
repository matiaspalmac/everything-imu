//! GitHub-release auto-update via `tauri-plugin-updater`.
//!
//! Reads the signed `latest.json` manifest published alongside each
//! GitHub release and — when newer than the running build — downloads
//! the platform bundle (NSIS on Windows, AppImage/deb on Linux) and
//! hands install off to the OS. The plugin verifies the bundle's
//! minisign signature against the embedded public key before applying.

use serde::{Deserialize, Serialize};
use tauri_plugin_updater::UpdaterExt;
use thiserror::Error;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub url: String,
    pub update_available: bool,
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("updater: {0}")]
    Backend(#[from] tauri_plugin_updater::Error),
    #[error("no update available")]
    NoUpdate,
}

/// Query the signed update manifest. Network-only; no files are touched.
pub async fn check(app: tauri::AppHandle) -> Result<UpdateInfo, UpdateError> {
    let maybe = app.updater()?.check().await?;
    match maybe {
        Some(update) => Ok(UpdateInfo {
            current: CURRENT_VERSION.to_string(),
            latest: update.version.clone(),
            url: update.download_url.to_string(),
            update_available: true,
        }),
        None => Ok(UpdateInfo {
            current: CURRENT_VERSION.to_string(),
            latest: CURRENT_VERSION.to_string(),
            url: String::new(),
            update_available: false,
        }),
    }
}

/// Download the signed bundle for the current OS+arch and run the
/// platform installer. Caller is expected to invoke `app.restart()` on
/// success; on Windows NSIS `installMode: passive`, the installer relaunches
/// the app itself and this function returns after the install kicks off.
pub async fn apply(app: tauri::AppHandle) -> Result<UpdateInfo, UpdateError> {
    let update = app.updater()?.check().await?.ok_or(UpdateError::NoUpdate)?;
    let latest = update.version.clone();
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await?;
    Ok(UpdateInfo {
        current: CURRENT_VERSION.into(),
        latest,
        url: String::new(),
        update_available: false,
    })
}

//! Managed Tauri state.

use everything_imu_core::AppState;
use osc_haptics::HapticConfig;
use persistence::PersistenceDb;
use std::sync::Arc;
use tokio::sync::watch;

pub struct AppHandle {
    pub state: Arc<AppState>,
    pub db: Arc<PersistenceDb>,
    pub log_buffer: Arc<parking_lot::Mutex<std::collections::VecDeque<crate::dto::LogEntryDto>>>,
    /// Pushes live config to the running OSC haptic bridge. The haptics
    /// commands write here so changes apply without a restart.
    pub haptic_config_tx: watch::Sender<HapticConfig>,
}

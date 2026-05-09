//! Managed Tauri state.

use everything_imu_core::AppState;
use persistence::PersistenceDb;
use std::sync::Arc;

pub struct AppHandle {
    pub state: Arc<AppState>,
    pub db: Arc<PersistenceDb>,
    pub log_buffer: Arc<parking_lot::Mutex<std::collections::VecDeque<crate::dto::LogEntryDto>>>,
}

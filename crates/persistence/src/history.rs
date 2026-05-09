//! Device history row exposed to UI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceHistoryRow {
    pub mac: [u8; 6],
    pub serial: String,
    pub kind: String,
    pub last_seen: i64,
    pub rotation_deg: f32,
}

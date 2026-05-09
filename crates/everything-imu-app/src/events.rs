//! Specta-typed events emitted to the UI.

use crate::dto::{DeviceHistoryDto, DeviceMetadataDto};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct TrackerSnapshot {
    pub mac: [u8; 6],
    pub serial: String,
    pub quat_xyzw: [f32; 4],
    pub battery_fraction: f32,
    pub rate_hz: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct TrackerUpdate {
    pub trackers: Vec<TrackerSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct DeviceDiscovered {
    pub metadata: DeviceMetadataDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DeviceConnState {
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct DeviceStateChanged {
    pub mac: [u8; 6],
    pub state: DeviceConnState,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct LogEntry {
    pub ts_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct DeviceHistoryUpdated {
    pub rows: Vec<DeviceHistoryDto>,
}

/// One raw IMU sample per known device at the emitter cadence (~30 Hz).
/// Frame is the device-native body frame.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ImuSampleEntry {
    pub mac: [u8; 6],
    pub gyr_xyz: [f32; 3],
    pub acc_xyz: [f32; 3],
    pub mag_xyz: Option<[f32; 3]>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct ImuSampleUpdate {
    pub samples: Vec<ImuSampleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct BiasEntry {
    pub mac: [u8; 6],
    pub gyr_bias: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct BiasUpdate {
    pub entries: Vec<BiasEntry>,
}

/// Snapshot of the SlimeClient runtime state for the Connection panel.
/// Emitted ~1 Hz and also returned synchronously by the
/// `get_connection_status` command for first paint.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct ConnectionStatusUpdate {
    pub server_addr: String,
    pub server_supports_bundle: bool,
    pub packets_sent: u64,
    pub last_send_ms_ago: Option<u64>,
    pub last_handshake_ms_ago: Option<u64>,
}

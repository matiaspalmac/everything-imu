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

/// Per-tracker latency / jitter snapshot for the diagnostics panel.
/// Pure bridge telemetry — no fusion or motion data, just how the bridge
/// itself is performing (inter-batch interval, UDP send call duration,
/// rough drop estimate from interval gaps).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LatencyEntry {
    pub mac: [u8; 6],
    pub interval_us_p50: f32,
    pub interval_us_p95: f32,
    pub interval_us_p99: f32,
    pub jitter_us: f32,
    pub send_us_p50: f32,
    pub send_us_p95: f32,
    pub dropped_estimate: u32,
    pub samples_window: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct LatencyUpdate {
    pub entries: Vec<LatencyEntry>,
}

/// Snapshot of the SlimeClient runtime state for the Connection panel.
/// Emitted ~1 Hz and also returned synchronously by the
/// `get_connection_status` command for first paint.
/// A distinct OSC address the haptic bridge has observed from VRChat.
/// The haptics config UI listens for these so the user can tap an avatar
/// contact in-game and bind the address that lights up.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct HapticAddressDiscovered {
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct ConnectionStatusUpdate {
    pub server_addr: String,
    pub server_supports_bundle: bool,
    pub packets_sent: u64,
    pub last_send_ms_ago: Option<u64>,
    pub last_handshake_ms_ago: Option<u64>,
}

/// Lifecycle event for the boot-time + manual updater. The UI listens
/// to drive a small toast/banner — `Checking → Available → Installing →
/// Installed` for the happy path; `NoUpdate` for the quiet path; the
/// `message` field surfaces backend errors verbatim on `Failed`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "stage")]
pub enum UpdateStage {
    Checking,
    NoUpdate { current: String },
    Available { current: String, latest: String },
    Installing { current: String, latest: String },
    Installed { latest: String },
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct UpdateStatus {
    pub stage: UpdateStage,
}

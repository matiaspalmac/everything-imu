//! Specta-typed DTO mirrors of internal types.

use device_traits::ResetKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DeviceMetadataDto {
    pub mac: [u8; 6],
    pub serial: String,
    pub kind: String,
    pub firmware: Option<String>,
    pub has_magnetometer: bool,
    pub has_battery: bool,
    pub has_rumble: bool,
    pub native_imu_rate_hz: u16,
}

impl From<&device_traits::DeviceMetadata> for DeviceMetadataDto {
    fn from(m: &device_traits::DeviceMetadata) -> Self {
        Self {
            mac: m.id.mac,
            serial: m.id.serial.clone(),
            kind: format!("{:?}", m.kind),
            firmware: m.firmware.clone(),
            has_magnetometer: m.capabilities.has_magnetometer,
            has_battery: m.capabilities.has_battery,
            has_rumble: m.capabilities.has_rumble,
            native_imu_rate_hz: m.capabilities.native_imu_rate_hz,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DeviceHistoryDto {
    pub mac: [u8; 6],
    pub serial: String,
    pub kind: String,
    pub last_seen: i64,
    pub rotation_deg: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SettingsDto {
    pub slime_server_addr: String,
    pub log_filter: String,
    pub theme: String,
    pub auto_start_synthetic: bool,
    /// When true, clicking the window close button hides the window to
    /// the tray instead of exiting. Quit is still reachable via the
    /// tray menu's Quit entry. Default false to match pre-tray behavior.
    pub close_to_tray: bool,
}

impl Default for SettingsDto {
    fn default() -> Self {
        Self {
            slime_server_addr: "127.0.0.1:6969".into(),
            log_filter: "info".into(),
            theme: "dark".into(),
            auto_start_synthetic: false,
            close_to_tray: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ResetKindDto {
    Yaw,
    Full,
    Mounting,
}

impl From<ResetKindDto> for ResetKind {
    fn from(k: ResetKindDto) -> Self {
        match k {
            ResetKindDto::Yaw => ResetKind::Yaw,
            ResetKindDto::Full => ResetKind::Full,
            ResetKindDto::Mounting => ResetKind::Mounting,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LogEntryDto {
    pub ts_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
}

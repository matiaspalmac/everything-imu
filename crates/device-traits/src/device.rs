//! Device trait + identity types.

use crate::events::ChannelInfo;
use std::fmt;
use tokio::sync::mpsc;

/// Stable device identity. The `mac` is what SlimeVR-Server uses to disambiguate
/// trackers across reconnects. For devices without a real MAC (USB-only DS4),
/// derive a stable hash from VID:PID + serial.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId {
    pub mac: [u8; 6],
    pub serial: String,
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}/{}",
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5],
            self.serial
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    JoyConL,
    JoyConR,
    ProController,
    ChargingGripL,
    ChargingGripR,
    JoyCon2L,
    JoyCon2R,
    ProController2,
    DualSense,
    DualSenseEdge,
    DualShock4,
    PsMoveZcm1,
    PsMoveZcm2,
    Wii,
    Tesla,
    SteamDeck,
    SteamController,
    Hopx,
    ThreeDs,
    Vita,
    DualShock3,
    /// Android phone forwarded via the eimu remote protocol.
    Phone,
    /// Wear OS watch forwarded via the eimu remote protocol.
    Watch,
    /// Generic motion-capable gamepad forwarded via the eimu remote protocol
    /// (read through Android's InputDevice sensor API, exact model unknown).
    Gamepad,
}

#[derive(Debug, Clone)]
pub struct DeviceMetadata {
    pub id: DeviceId,
    pub kind: DeviceKind,
    pub firmware: Option<String>,
    pub capabilities: DeviceCapabilities,
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceCapabilities {
    pub has_magnetometer: bool,
    pub has_battery: bool,
    pub has_rumble: bool,
    pub native_imu_rate_hz: u16,
}

#[derive(thiserror::Error, Debug)]
pub enum DeviceError {
    #[error("HID transport error: {0}")]
    Hid(String),
    #[error("device disconnected")]
    Disconnected,
    #[error("calibration parse failed: {0}")]
    Calibration(String),
    #[error("subcommand timed out: {0}")]
    Timeout(String),
}

/// Object-safe device trait. Each device owns its background reader (typically a
/// dedicated `std::thread`) and emits events via mpsc.
#[async_trait::async_trait]
pub trait Device: Send {
    fn metadata(&self) -> &DeviceMetadata;

    /// Spawn the device's reader pipeline. Caller owns the receiver — drop to stop.
    /// Idempotent: a second call without `stop()` first should return `DeviceError::Hid`.
    async fn start(&mut self) -> Result<mpsc::Receiver<ChannelInfo>, DeviceError>;

    async fn stop(&mut self) -> Result<(), DeviceError>;

    /// Set player-LED mask (1 = solid, 0 = off; bits [3:0] = LED1..4).
    async fn set_led_mask(&mut self, mask: u8) -> Result<(), DeviceError>;

    /// Set rumble intensity in `0.0..=1.0` (0.0 = off). Drivers convert this
    /// to their hardware's amplitude representation; see `crate::rumble`.
    async fn set_rumble(&mut self, intensity: f32) -> Result<(), DeviceError>;
}

/// Long-running enumerate task that emits `(metadata, device)` pairs as they appear.
#[async_trait::async_trait]
pub trait DeviceFactory: Send + Sync {
    async fn enumerate_loop(
        &self,
        out: mpsc::Sender<(DeviceMetadata, Box<dyn Device>)>,
    ) -> Result<(), DeviceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_display_format() {
        let id = DeviceId {
            mac: [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE],
            serial: "TEST".into(),
        };
        assert_eq!(format!("{id}"), "DE:AD:BE:EF:CA:FE/TEST");
    }
}

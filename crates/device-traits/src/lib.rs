//! Abstract device interface for the everything-imu IMU bridge.
//!
//! Every device (Joy-Con, DualSense, PS Move, etc.) implements [`Device`] and is
//! emitted by a [`DeviceFactory`]. Devices stream [`ChannelInfo`] events through a
//! tokio mpsc channel; the consumer (in `core::Pipeline`) feeds samples into fusion
//! and forwards reset / battery / disconnect events to SlimeVR-Server.

pub mod device;
pub mod events;
pub mod health;
pub mod reset;
pub mod stores;

#[cfg(feature = "mock")]
pub mod mock;

pub use device::{
    Device, DeviceCapabilities, DeviceError, DeviceFactory, DeviceId, DeviceKind, DeviceMetadata,
};
pub use events::{BatteryState, ChannelInfo, ImuSample};
pub use health::{DeviceHealth, HealthClassifier};
pub use reset::{ButtonState, ResetButtonDetector, ResetKind};
pub use stores::{BiasStore, InMemoryBiasStore, InMemorySettingsStore, SettingsStore};

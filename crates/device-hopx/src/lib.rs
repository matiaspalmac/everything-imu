//! BLE IMU tracker driver (Nordic UART Service transport).
//!
//! Targets a compact nRF52-based motion puck that exposes a 6-axis IMU over the
//! Nordic UART Service (NUS). The device packs accelerometer + gyroscope samples
//! into fixed 14-byte records and streams them as GATT notifications; this crate
//! reassembles those records, scales them to SI units, and emits
//! [`device_traits::ImuSample`] events for the everything-imu pipeline — fusion
//! (VQF) and SlimeVR forwarding happen downstream like every other driver.
//!
//! Layout mirrors the other device crates:
//! - [`protocol`] — pure, fully-tested wire logic (NUS UUIDs, start/stop
//!   commands, raw→SI scales, advertised-name matching, the streaming record
//!   parser, and the body-frame axis remap hook).
//! - [`device`] — [`device_traits::Device`] impl wrapping the BLE transport.
//! - [`factory`] — [`device_traits::DeviceFactory`] impl that scans for the
//!   tracker by advertised name and emits a device per unit.

pub mod device;
pub mod diagnostics;
pub mod factory;
pub mod protocol;

pub use device::HopxDevice;
pub use device_traits::{Device, DeviceFactory};
pub use factory::{scan_nearby, HopxFactory, NearbyHopx};

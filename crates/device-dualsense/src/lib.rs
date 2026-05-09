//! Sony controller IMU driver — DualSense (PS5) + DualSense Edge + DualShock 4 (PS4).
//!
//! Wire format reference:
//! - DualSense USB input report 0x01 (64 bytes after report ID is stripped by hidapi).
//! - DualSense BT input report 0x31 (78 bytes, payload offset by 1 vs USB) — recognized
//!   but treated as "compatibility-mode 0x01 with shifted payload" for now. Full BT
//!   pairing handshake / CRC32 is queued for a follow-up sprint.
//! - DualShock 4 USB input report 0x01 (10 bytes header + IMU at offset 13).
//!
//! IMU calibration:
//! - Default scale factors come from pydualsense / Sony PSDK reference. ±2000 deg/s gyro
//!   range and ±4 g accel range are factory defaults; per-device calibration via feature
//!   report 0x05 is read on connect when available, otherwise fallback to defaults.
//!
//! This crate intentionally does NOT touch output reports (LEDs / triggers / haptics).
//! The bridge only forwards IMU motion to SlimeVR-Server; cosmetic feedback is out of scope.
#![allow(dead_code)]

pub mod ids;

pub(crate) mod hid;
pub(crate) mod report;

pub mod device;
pub mod factory;

#[cfg(feature = "synthetic-source")]
pub mod synthetic;

pub use device::DualSenseDevice;
pub use device_traits::{Device, DeviceFactory};
pub use factory::{DualSenseFactory, PairedDualSense};
pub use ids::{ControllerKind, SONY_VID};
#[cfg(feature = "synthetic-source")]
pub use synthetic::SyntheticDualSense;

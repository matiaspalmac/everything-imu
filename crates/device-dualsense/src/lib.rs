//! Sony controller IMU driver: DualSense (PS5), DualSense Edge, and DualShock 4 (PS4).
//!
//! Wire format (hidapi keeps the report ID at buf[0]):
//! - DualSense USB input report 0x01 (64 bytes): gyro at 16, accel at 22.
//! - DualSense BT input report 0x31: a single sequence-tag byte at buf[1] shifts
//!   the payload +1 vs USB, so gyro is at 17 and accel at 23. Bluetooth output
//!   reports carry a trailing CRC32.
//! - DualShock 4 USB input report 0x01 (64 bytes): gyro at 13, accel at 19.
//! - DualShock 4 BT input report 0x11: a 2-byte header shifts the payload +2 (gyro
//!   15, accel 21). Windows delivers this report padded, so the parser matches any
//!   length of at least 78 bytes rather than an exact size.
//!
//! IMU calibration:
//! - Default scale factors come from pydualsense / Sony PSDK reference. ±2000 deg/s gyro
//!   range and ±4 g accel range are factory defaults; per-device calibration via feature
//!   report 0x05 is read on connect when available, otherwise fallback to defaults.
//!
//! Output reports are supported for DualSense / DualSense Edge / DualShock 4
//! basic feedback (player LED + rumble).
#![allow(dead_code)]

pub mod ids;

pub(crate) mod axis_remap;
pub(crate) mod hid;
pub(crate) mod report;

pub mod device;
pub mod diagnostics;
pub mod factory;

#[cfg(feature = "synthetic-source")]
pub mod synthetic;

pub use device::DualSenseDevice;
pub use device_traits::{Device, DeviceFactory};
pub use factory::{DualSenseFactory, PairedDualSense};
pub use ids::{ControllerKind, SONY_VID};
#[cfg(feature = "synthetic-source")]
pub use synthetic::SyntheticDualSense;

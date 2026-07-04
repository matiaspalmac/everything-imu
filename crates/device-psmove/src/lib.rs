//! PlayStation Move motion controller driver — ZCM1 (PS3) + ZCM2 (PS4 refresh).
//!
//! Wire reference: `docs/reference/psmove_protocol.md` (and the PS Dev Wiki
//! Move entries). Both ZCM1 and ZCM2 expose a 49-byte HID input report 0x01.
//! ZCM1 has a 3-axis magnetometer; ZCM2 dropped the mag and bumped the
//! gyro full-scale range. Each report packs *two* IMU frames (sub-rate
//! doubling), so we emit two `ImuSample`s per parse. Accel/gyro fields are
//! little-endian u16 (matching `report.rs`), like the DualSense / DS4 path.
//!
//! Output reports are supported for sphere LED color + rumble (report 0x06).
#![allow(dead_code)]

pub mod ids;

pub(crate) mod hid;
pub(crate) mod report;

pub mod axis_remap;
pub mod calibration;
pub mod device;
pub mod diagnostics;
pub mod factory;
pub mod pairing;

#[cfg(feature = "synthetic-source")]
pub mod synthetic;

pub use device::PsMoveDevice;
pub use device_traits::{Device, DeviceFactory};
pub use factory::{PairedPsMove, PsMoveFactory};
pub use ids::{ControllerKind, SONY_VID};
#[cfg(feature = "synthetic-source")]
pub use synthetic::{SyntheticPsMove, SYNTH_MAG_OFFSET};

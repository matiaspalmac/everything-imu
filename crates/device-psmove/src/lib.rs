//! PlayStation Move motion controller driver — ZCM1 (PS3) + ZCM2 (PS4 refresh).
//!
//! Wire reference: psmoveapi `psmove.c` and the PS Dev Wiki entries for
//! the Move. Both ZCM1 and ZCM2 expose a 49-byte HID input report 0x01.
//! ZCM1 has a 3-axis magnetometer; ZCM2 dropped the mag and bumped the
//! gyro full-scale range. Each report packs *two* IMU frames (sub-rate
//! doubling), so we emit two `ImuSample`s per parse. Big-endian
//! throughout — distinct from the DualSense / DS4 little-endian path.
//!
//! Output reports (sphere LED color, rumble level) are intentionally
//! out of scope: the bridge only forwards motion to SlimeVR-Server.
#![allow(dead_code)]

pub mod ids;

pub(crate) mod hid;
pub(crate) mod report;

pub mod device;
pub mod factory;

#[cfg(feature = "synthetic-source")]
pub mod synthetic;

pub use device::PsMoveDevice;
pub use device_traits::{Device, DeviceFactory};
pub use factory::{PairedPsMove, PsMoveFactory};
pub use ids::{ControllerKind, SONY_VID};
#[cfg(feature = "synthetic-source")]
pub use synthetic::SyntheticPsMove;

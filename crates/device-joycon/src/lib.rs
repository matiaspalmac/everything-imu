//! Joy-Con 1 (L/R), Switch Pro Controller, and Charging Grip HID protocol.
//!
//! Sprint 3 ships protocol surface ahead of full consumer wiring (Sprint 3c + hardware
//! smoke). Several SPI-cal items are deliberately built-but-not-yet-consumed; relax
//! dead-code lints crate-wide for this phase.
#![allow(dead_code)]

pub mod ids;

pub(crate) mod axis_remap;
pub(crate) mod calibration;
pub(crate) mod hid;
pub(crate) mod jc1;
pub(crate) mod jc2;
pub(crate) mod report;
pub(crate) mod reset_buttons;
pub(crate) mod spi_cal;
pub(crate) mod subcmd;

#[cfg(feature = "synthetic-source")]
pub mod synthetic;

pub mod factory;

pub use device_traits::{Device, DeviceFactory};
pub use factory::{JoyconFactory, PairedJoycon};
pub use ids::{ControllerKind, JOYCON_VID};

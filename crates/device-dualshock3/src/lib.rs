//! DualShock 3 / SIXAXIS (PS3) device driver — **experimental**.
//!
//! The DS3 carries a 3-axis accelerometer but only a **single-axis (yaw)
//! gyroscope** and no magnetometer. It cannot produce drift-free full
//! orientation: pitch/roll come from gravity, yaw drifts unconstrained. Ship it
//! behind a clear UI warning; it is included for completeness, not because it is
//! a good tracker. See `docs/reference/dualshock3_protocol.md`.
//!
//! Transport: USB HID via hidapi. The pad must be told to start streaming with a
//! feature-report handshake before input report `0x01` flows.

mod device;
mod factory;
mod report;

pub use device::DualShock3Device;
pub use factory::DualShock3Factory;
pub use report::{parse_input_report, Ds3Motion};

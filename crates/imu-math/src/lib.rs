//! Pure math utilities for IMU pipelines: coordinate frame transforms and quaternion helpers.
//!
//! Single source of truth for math types — re-exports from nalgebra so downstream crates
//! consume `imu_math::Vector3` rather than `nalgebra::Vector3`. Allows future math-lib swap
//! without touching device-* crates.

pub use nalgebra::{Quaternion, UnitQuaternion, Vector3};

pub mod coord;
pub mod mag_cal;
pub mod mag_cal_wizard;
pub mod quat;

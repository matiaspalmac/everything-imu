//! IMU fusion algorithms: VQF and Madgwick AHRS.
//!
//! Concrete types only — no `FusionAlgorithm` trait. Devices pick the implementation
//! that matches their hardware capabilities (6-axis vs 9-axis).

pub use imu_math::{Quaternion, UnitQuaternion, Vector3};

pub mod basic_vqf;
pub mod madgwick;
pub mod vqf;

pub use basic_vqf::BasicVqf;
pub use madgwick::Madgwick;
pub use vqf::{Vqf, VqfParams};

//! BasicVQF — 6D-only minimal subset (no bias estimation, no rest detection, no mag rejection).
//!
//! Implemented as a thin wrapper around [`crate::vqf::Vqf`] with the bias-estimation,
//! rest-detection, and mag-rejection feature flags disabled. For 6-axis devices
//! (Joy-Con 1, Joy-Con 2, DualSense, DualShock 4, PS Move ZCM2) where yaw drift
//! is structural and there is no magnetometer to reject disturbances against.
//!
//! BasicVqf is a 6-axis (magnetometer-less) orientation filter. It is retained as a
//! user-selectable fusion algorithm in the desktop app and kept in this shared crate
//! for cross-target parity. The mobile JNI fusion path instead runs the full
//! [`crate::vqf::Vqf`], which subsumes the 6-axis case (motion-bias estimation is
//! auto-disabled when no magnetometer is present), so BasicVqf is intentionally not
//! instantiated on mobile.

use crate::vqf::{Vqf, VqfParams};

pub struct BasicVqf {
    inner: Vqf,
}

impl BasicVqf {
    pub fn new(gyr_ts: f64) -> Self {
        let params = VqfParams {
            motion_bias_est_enabled: false,
            rest_bias_est_enabled: false,
            mag_dist_rejection_enabled: false,
            ..VqfParams::default()
        };
        Self {
            inner: Vqf::with_params(gyr_ts, params),
        }
    }

    pub fn update(&mut self, gyro: [f64; 3], accel: [f64; 3]) {
        self.inner.update(gyro, accel, None);
    }

    pub fn quat_6d(&self) -> [f64; 4] {
        self.inner.quat_6d()
    }

    pub fn reset_state(&mut self) {
        self.inner.reset_state();
    }

    /// Update the sample timestep live, preserving the orientation estimate.
    /// See [`Vqf::set_timestep`].
    pub fn set_timestep(&mut self, gyr_ts: f64) {
        self.inner.set_timestep(gyr_ts);
    }
}

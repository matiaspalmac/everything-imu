//! BasicVQF — 6D-only minimal subset (no bias estimation, no rest detection, no mag rejection).
//!
//! Implemented as a thin wrapper around [`crate::vqf::Vqf`] with the bias-estimation,
//! rest-detection, and mag-rejection feature flags disabled. For 6-axis devices
//! (Joy-Con 1, Joy-Con 2, DualSense, DualShock 4, PS Move ZCM2) where yaw drift
//! is structural and there is no magnetometer to reject disturbances against.

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
}

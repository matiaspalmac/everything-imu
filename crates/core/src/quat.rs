//! Quaternion type for the UI surface.

use serde::{Deserialize, Serialize};

/// Hamilton quaternion in `[i, j, k, w]` order (three.js compatible —
/// `quat.fromArray(arr)` consumes `[x, y, z, w]` directly).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct QuatXyzw(pub [f32; 4]);

impl QuatXyzw {
    pub const IDENTITY: Self = Self([0.0, 0.0, 0.0, 1.0]);

    /// Build from `Vqf::quat_6d()` `[w, i, j, k]` f64.
    pub fn from_vqf_wijk(q: [f64; 4]) -> Self {
        Self([q[1] as f32, q[2] as f32, q[3] as f32, q[0] as f32])
    }
}

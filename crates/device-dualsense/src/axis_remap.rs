//! Per-`ControllerKind` axis remap bringing the chip-native DS4/DS5 IMU frame
//! into the "JSL Y-up" convention the pipeline expects: X right, Y up (out of the
//! controller face, through the touchpad), Z toward the player.
//!
//! Downstream, `core::pipeline` feeds the remapped sample through
//! `imu_math::coord::jsl_to_vqf_body`, which maps JSL `(x, y, z)` → VQF body
//! `(x, -z, y)` (VQF integrates in a Z-up body frame). So whatever this function
//! emits on JSL +Y lands on VQF +Z, where VQF expects gravity at rest.
//!
//! ## DualSense / DualSense Edge — empirically identity (verified 2026-05-30)
//!
//! Live `--ds-raw` capture on a real DualSense (USB, pid 0x0CE6), held flat and
//! face-up, reads accel ≈ `[+100, +7935, +1260]` LSB — gravity dominant on the
//! chip **+Y** axis, magnitude ≈ 8192 LSB/g (±4 g scale). The DS5 IMU is mounted
//! in the SDL-standard gamepad frame (X right, Y up, Z toward player), which is
//! exactly the JSL Y-up convention. Passing it through unchanged sends gravity to
//! JSL +Y → VQF +Z, the correct rest attitude (no gimbal lock). Hence identity is
//! the *correct* transform for DS5, not a placeholder.
//!
//! ## DualShock 4 — identity, hardware-validation pending
//!
//! No DS4 hardware was available to capture this session. The DS4 IMU is mounted
//! in the same physical orientation and reported in the same SDL-standard frame
//! per `hid-playstation.c`, so identity is the reference-correct default. Flagged
//! validation-pending: re-confirm with `--ds-raw` over a DS4 before relying on it,
//! and add the sign/permutation correction here if the live frame differs.

use crate::ids::ControllerKind;

/// Map a chip-frame IMU vector (gyro rad/s or accel m/s²) into JSL Y-up.
///
/// Both Sony families currently report in the SDL-standard frame that already
/// matches JSL Y-up, so this is identity. The per-kind match is retained as the
/// single place to encode a real permutation/sign fix should a future device or
/// firmware report a rotated frame.
#[inline]
pub fn apply(kind: ControllerKind, v: [f32; 3]) -> [f32; 3] {
    match kind {
        // DS5/Edge: chip frame == JSL Y-up (gravity on +Y at rest, live-verified).
        ControllerKind::DualSense | ControllerKind::DualSenseEdge => v,
        // DS4: same reference frame; validation pending (no hardware this session).
        ControllerKind::DualShock4 => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At rest, gravity reads on chip +Y (live DS5 capture). After the remap it
    /// must still be on +Y so that `jsl_to_vqf_body` lands it on VQF +Z.
    #[test]
    fn ds5_rest_gravity_stays_on_y() {
        let g = apply(ControllerKind::DualSense, [0.1, 9.806, 0.12]);
        assert!(g[1] > 9.0, "gravity must remain on +Y, got {g:?}");
        assert!(g[0].abs() < 1.0 && g[2].abs() < 1.0);
    }

    #[test]
    fn identity_preserves_all_components_ds5() {
        assert_eq!(apply(ControllerKind::DualSense, [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
        assert_eq!(
            apply(ControllerKind::DualSenseEdge, [1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn identity_preserves_all_components_ds4() {
        assert_eq!(
            apply(ControllerKind::DualShock4, [-1.0, 2.0, -3.0]),
            [-1.0, 2.0, -3.0]
        );
    }
}

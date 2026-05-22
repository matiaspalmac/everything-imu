//! Per-ControllerKind axis remap bringing chip-native DS4/DS5 BMI frame into the
//! "JSL Y-up" convention (X right, Y up through buttons, Z forward toward player).
//!
//! Hid-playstation reports raw chip axes where +Z points out the top of the
//! controller (face-up on table → accel +Z ≈ +g). Downstream `jsl_to_vqf_body`
//! expects Y-up input; feeding chip-native Z-up through it lands gravity on
//! VQF body -Y, dropping the filter into a near-90° tilt at rest. Symptom: yaw
//! Euler wraps ±180° rapidly while the controller is stationary (gimbal lock).
//!
//! Mapping chip→JSL Y-up: rotate -90° about X. (x, y, z) → (x, z, -y).

use crate::ids::ControllerKind;

#[inline]
pub fn apply(kind: ControllerKind, v: [f32; 3]) -> [f32; 3] {
    // Temporary: identity passthrough. Empirical chip-frame mapping pending
    // raw-sample logging. Downstream `jsl_to_vqf_body` will treat input as
    // JSL Y-up; if real chip frame differs, we'll fix this in a follow-up.
    let _ = kind;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passthrough_during_empirical_fit() {
        let v = apply(ControllerKind::DualSense, [1.0, 2.0, 3.0]);
        assert_eq!(v, [1.0, 2.0, 3.0]);
    }
}

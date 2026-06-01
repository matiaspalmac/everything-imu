//! Wire-order axis remap for the PS Move IMU.
//!
//! The Move streams its accelerometer / gyroscope / magnetometer triplets in
//! physical order **X, Z, Y** on the wire (see `docs/ref_psmove_protocol.md`).
//! The fusion pipeline expects **X, Y, Z** in
//! the JSL Y-up convention (same target frame the DualSense remap feeds), so the
//! 2nd and 3rd components are swapped here to restore X-Y-Z.
//!
//! ## Handedness / signs — theory default, hardware-validation pending
//!
//! A bare swap of two axes inverts the handedness of a right-handed frame. The
//! *physically correct* remap (which component, if any, must also be negated to
//! keep gravity on the expected pipeline axis at rest) depends on the Move's IMU
//! mounting and can only be pinned down from a live still+rotate capture
//! (`--psmove-raw`). No Move hardware was available this session, so signs are
//! left identity and the deswizzle is documented as the single place to encode a
//! sign/permutation fix once a capture exists. This mirrors how the DualSense
//! `axis_remap` carried DS4 as validation-pending.

use crate::ids::ControllerKind;

/// Re-order a wire-frame IMU triplet (X-Z-Y) into pipeline order (X-Y-Z).
///
/// Applies to accelerometer, gyroscope, and magnetometer alike — all three
/// share the Move's single IMU mounting, so the same deswizzle holds. ZCM1 and
/// ZCM2 use the same physical layout, hence `kind` is currently unused but kept
/// so a future per-model correction has a home.
#[inline]
pub fn deswizzle(kind: ControllerKind, v: [f32; 3]) -> [f32; 3] {
    let _ = kind;
    // X stays; physical Z (wire slot 1) and Y (wire slot 2) swap back to Y, Z.
    [v[0], v[2], v[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deswizzle_swaps_second_and_third_components() {
        // Wire order X-Z-Y = [x, z, y] must come out X-Y-Z = [x, y, z].
        assert_eq!(
            deswizzle(ControllerKind::Zcm1, [1.0, 3.0, 2.0]),
            [1.0, 2.0, 3.0]
        );
        assert_eq!(
            deswizzle(ControllerKind::Zcm2, [10.0, 30.0, 20.0]),
            [10.0, 20.0, 30.0]
        );
    }

    #[test]
    fn deswizzle_is_its_own_inverse() {
        let v = [1.0, 2.0, 3.0];
        let once = deswizzle(ControllerKind::Zcm1, v);
        assert_eq!(deswizzle(ControllerKind::Zcm1, once), v);
    }
}

//! Quaternion helpers.

use crate::{Quaternion, UnitQuaternion, Vector3};

/// Normalize a quaternion. Falls back to identity when norm < 1e-6 (matches C# `QuaternionUtils.Normalize`).
pub fn normalize_or_identity(q: Quaternion<f32>) -> UnitQuaternion<f32> {
    let mag_sq = q.w * q.w + q.i * q.i + q.j * q.j + q.k * q.k;
    if mag_sq < 1e-12 {
        return UnitQuaternion::identity();
    }
    UnitQuaternion::from_quaternion(q)
}

/// Decompose a unit quaternion into (axis, angle in radians). Mirrors C#
/// `QuaternionUtils.ToAxisAngle`: axis fallback `(1, 0, 0)` if `s < 1e-4`.
pub fn axis_angle(q: UnitQuaternion<f32>) -> (Vector3<f32>, f32) {
    let qw = q.w.clamp(-1.0, 1.0);
    let angle = 2.0 * qw.acos();
    let s = (1.0 - qw * qw).sqrt();
    if s < 1e-4 {
        return (Vector3::new(1.0, 0.0, 0.0), angle);
    }
    (Vector3::new(q.i / s, q.j / s, q.k / s), angle)
}

/// Local rotation of child relative to parent — `normalize(parent⁻¹ · child)`.
/// Mirrors C# `QuaternionUtils.LocalRotation`.
pub fn local_rotation(
    child: UnitQuaternion<f32>,
    parent: UnitQuaternion<f32>,
) -> UnitQuaternion<f32> {
    let q = parent.inverse() * child;
    UnitQuaternion::from_quaternion(*q.quaternion())
}

/// Build a quaternion that rotates the reference vector (0, 0, -1) to the calibrated gravity
/// direction. Mirrors C# `QuaternionUtils.QuatFromGravity` (used for JC1 SPI cal helper).
pub fn quat_from_gravity(
    raw: Vector3<f32>,
    center: Vector3<f32>,
    scale: f32,
) -> UnitQuaternion<f32> {
    let scale = if scale.abs() < 1e-9 { 1.0 } else { scale };
    let s = (raw - center) / scale;
    let s = Vector3::new(
        s.x.clamp(-1.0, 1.0),
        s.y.clamp(-1.0, 1.0),
        s.z.clamp(-1.0, 1.0),
    );
    let gravity = match s.try_normalize(1e-6) {
        Some(g) => g,
        None => return UnitQuaternion::identity(),
    };
    let reference = Vector3::new(0.0, 0.0, -1.0);
    let dot = gravity.dot(&reference).clamp(-1.0, 1.0);
    if (dot - 1.0).abs() < 1e-5 {
        return UnitQuaternion::identity();
    }
    if (dot + 1.0).abs() < 1e-5 {
        // 180° about X
        return UnitQuaternion::from_quaternion(Quaternion::new(0.0, 1.0, 0.0, 0.0));
    }
    let axis = gravity.cross(&reference);
    let axis = match axis.try_normalize(1e-6) {
        Some(a) => a,
        None => return UnitQuaternion::identity(),
    };
    let angle = dot.acos();
    UnitQuaternion::from_axis_angle(&nalgebra::Unit::new_unchecked(axis), angle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn normalize_unit_passthrough() {
        let q = Quaternion::new(1.0, 0.0, 0.0, 0.0);
        let out = normalize_or_identity(q);
        assert_relative_eq!(out.w, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn normalize_zero_falls_back_to_identity() {
        let q = Quaternion::new(0.0, 0.0, 0.0, 0.0);
        let out = normalize_or_identity(q);
        assert_relative_eq!(out, UnitQuaternion::identity(), epsilon = 1e-6);
    }

    #[test]
    fn axis_angle_identity_zero_angle() {
        let q = UnitQuaternion::<f32>::identity();
        let (axis, angle) = axis_angle(q);
        assert_relative_eq!(angle, 0.0, epsilon = 1e-6);
        assert_relative_eq!(axis, Vector3::new(1.0, 0.0, 0.0), epsilon = 1e-6);
    }

    #[test]
    fn axis_angle_90_about_z() {
        use core::f32::consts::FRAC_PI_2;
        let q = UnitQuaternion::from_axis_angle(&nalgebra::Vector3::z_axis(), FRAC_PI_2);
        let (axis, angle) = axis_angle(q);
        assert_relative_eq!(angle, FRAC_PI_2, epsilon = 1e-6);
        assert_relative_eq!(axis.z, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn local_rotation_same_is_identity() {
        let q = UnitQuaternion::<f32>::identity();
        let out = local_rotation(q, q);
        assert_relative_eq!(out, q, epsilon = 1e-6);
    }

    #[test]
    fn quat_from_gravity_at_reference_is_identity() {
        let raw = Vector3::new(0.0_f32, 0.0, -1.0);
        let center = Vector3::zeros();
        let q = quat_from_gravity(raw, center, 1.0);
        assert_relative_eq!(q.w, 1.0, epsilon = 1e-5);
    }
}

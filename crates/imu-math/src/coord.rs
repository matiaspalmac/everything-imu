//! Coordinate frame transforms.
//! plus IMU-specific frame remaps (JSL→VQF body, VQF Z-up world→SlimeVR Y-up world).

use crate::{Quaternion, UnitQuaternion, Vector3};
use core::f32::consts::PI;

#[inline]
pub fn deg2rad(deg: f32) -> f32 {
    deg * (PI / 180.0)
}

#[inline]
pub fn rad2deg(rad: f32) -> f32 {
    rad * (180.0 / PI)
}

/// Build a unit quaternion from Euler XYZ angles in DEGREES (roll, pitch, yaw).
/// Mirrors SlimeIMU `CoordinateUtility.ToQuaternion(double x, double y, double z)` which
/// also accepts degrees despite parameter names — preserved for byte-parity.
pub fn euler_xyz_to_quat(roll_deg: f32, pitch_deg: f32, yaw_deg: f32) -> UnitQuaternion<f32> {
    let r = deg2rad(roll_deg);
    let p = deg2rad(pitch_deg);
    let y = deg2rad(yaw_deg);

    let cr = (r * 0.5).cos();
    let sr = (r * 0.5).sin();
    let cp = (p * 0.5).cos();
    let sp = (p * 0.5).sin();
    let cy = (y * 0.5).cos();
    let sy = (y * 0.5).sin();

    let w = cr * cp * cy + sr * sp * sy;
    let i = sr * cp * cy - cr * sp * sy;
    let j = cr * sp * cy + sr * cp * sy;
    let k = cr * cp * sy - sr * sp * cy;

    UnitQuaternion::from_quaternion(Quaternion::new(w, i, j, k))
}

/// Quaternion → Euler XYZ in DEGREES. Returns (roll, pitch, yaw).
/// Mirrors SlimeIMU `CoordinateUtility.QuaternionToEuler` which converts to degrees at the end.
pub fn quat_to_euler_xyz_deg(q: UnitQuaternion<f32>) -> Vector3<f32> {
    let qw = q.w;
    let qx = q.i;
    let qy = q.j;
    let qz = q.k;

    let sinr_cosp = 2.0 * (qw * qx + qy * qz);
    let cosr_cosp = 1.0 - 2.0 * (qx * qx + qy * qy);
    let roll = sinr_cosp.atan2(cosr_cosp);

    let sinp = 2.0 * (qw * qy - qz * qx);
    let pitch = if sinp.abs() >= 1.0 {
        sinp.signum() * core::f32::consts::FRAC_PI_2
    } else {
        sinp.asin()
    };

    let siny_cosp = 2.0 * (qw * qz + qx * qy);
    let cosy_cosp = 1.0 - 2.0 * (qy * qy + qz * qz);
    let yaw = siny_cosp.atan2(cosy_cosp);

    Vector3::new(rad2deg(roll), rad2deg(pitch), rad2deg(yaw))
}

/// Quaternion → Euler ZXY in DEGREES. Returns (pitch, yaw, roll).
/// Mirrors SlimeIMU `CoordinateUtility.QuaternionToEulerZXY`.
pub fn quat_to_euler_zxy_deg(q: UnitQuaternion<f32>) -> Vector3<f32> {
    let qw = q.w;
    let qx = q.i;
    let qy = q.j;
    let qz = q.k;

    let pitch = (2.0 * (qw * qy - qz * qx)).clamp(-1.0, 1.0).asin();

    if pitch.abs() >= core::f32::consts::FRAC_PI_2 {
        let yaw = qy.atan2(qw);
        return Vector3::new(rad2deg(pitch), rad2deg(yaw), 0.0);
    }

    let roll = (2.0 * (qw * qx + qy * qz)).atan2(1.0 - 2.0 * (qx * qx + qy * qy));
    let yaw = (2.0 * (qw * qz + qx * qy)).atan2(1.0 - 2.0 * (qy * qy + qz * qz));

    Vector3::new(rad2deg(pitch), rad2deg(yaw), rad2deg(roll))
}

/// Extract yaw (Y-axis rotation) directly from quaternion, returns DEGREES.
/// Mirrors SlimeIMU `CoordinateUtility.GetYawFromQuaternion`.
pub fn yaw_deg_from_quat(q: UnitQuaternion<f32>) -> f32 {
    let qw = q.w;
    let qx = q.i;
    let qy = q.j;
    let qz = q.k;
    let yaw = (2.0 * (qw * qy + qx * qz)).atan2(1.0 - 2.0 * (qy * qy + qz * qz));
    rad2deg(yaw)
}

/// Extract pitch (X-axis rotation) directly from quaternion, returns DEGREES.
/// Mirrors SlimeIMU `CoordinateUtility.GetXAxisFromQuaternion`.
pub fn pitch_deg_from_quat(q: UnitQuaternion<f32>) -> f32 {
    let qw = q.w;
    let qx = q.i;
    let qy = q.j;
    let qz = q.k;
    let pitch = (2.0 * (qw * qx + qy * qz)).atan2(1.0 - 2.0 * (qx * qx + qy * qy));
    rad2deg(pitch)
}

/// Apply rotation `q` to vector `v` — `v' = q · v · q⁻¹`. Mirrors SlimeIMU `Transform`.
pub fn transform_v(v: Vector3<f32>, q: UnitQuaternion<f32>) -> Vector3<f32> {
    q.transform_vector(&v)
}

/// Build a unit quaternion that rotates `from` to `to`. Mirrors SlimeIMU
/// `CalculateRotationQuaternion`. Returns identity for parallel/antiparallel inputs.
pub fn rotation_from_to(from: Vector3<f32>, to: Vector3<f32>) -> UnitQuaternion<f32> {
    let f = from.try_normalize(1e-6);
    let t = to.try_normalize(1e-6);
    let (f, t) = match (f, t) {
        (Some(f), Some(t)) => (f, t),
        _ => return UnitQuaternion::identity(),
    };

    let axis = f.cross(&t);
    let axis_len = axis.norm();
    if axis_len < 1e-6 {
        return UnitQuaternion::identity();
    }
    let axis = axis / axis_len;

    let dot = f.dot(&t).clamp(-1.0, 1.0);
    let angle = dot.acos();

    UnitQuaternion::from_axis_angle(&nalgebra::Unit::new_unchecked(axis), angle)
}

/// JSL right-handed Y-up body frame → VQF right-handed Z-up body frame.
/// Matrix: |1 0 0; 0 0 -1; 0 1 0|, det = +1, equivalent to +90° rotation about X.
/// Axis mapping: out.x = in.x, out.y = -in.z, out.z = in.y.
/// Cast f32 inputs to f64 since VQF state is f64.
///
/// Source-frame assumption: the upright-portrait Android sensor frame is treated as
/// the JSL identity pose, and this remap is applied uniformly to remote phone/watch
/// samples that arrive in the raw Android sensor frame. A landscape or watch mount
/// that does not match this basis will need a user recenter — this is a documented
/// known assumption, not a bug.
#[inline]
pub fn jsl_to_vqf_body(v: Vector3<f32>) -> [f64; 3] {
    [v.x as f64, -v.z as f64, v.y as f64]
}

/// VQF Z-up world → SlimeVR Y-up world. Pre-composes a -90° rotation about world X
/// onto the VQF output. Casts f64 → f32 (wire is f32 BE).
pub fn vqf_zup_world_to_slimevr_yup(q_zup: UnitQuaternion<f64>) -> UnitQuaternion<f32> {
    use core::f64::consts::FRAC_PI_4;
    let half_cos = FRAC_PI_4.cos();
    let half_sin = FRAC_PI_4.sin();
    // -90° about X: (cos(-45°), sin(-45°), 0, 0) = (cos45, -sin45, 0, 0)
    let world_fix =
        UnitQuaternion::<f64>::from_quaternion(Quaternion::new(half_cos, -half_sin, 0.0, 0.0));
    let composed = world_fix * q_zup;
    UnitQuaternion::<f32>::from_quaternion(Quaternion::new(
        composed.w as f32,
        composed.i as f32,
        composed.j as f32,
        composed.k as f32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn deg2rad_180_eq_pi() {
        assert_relative_eq!(deg2rad(180.0), PI, epsilon = 1e-6);
    }

    #[test]
    fn rad2deg_pi_eq_180() {
        assert_relative_eq!(rad2deg(PI), 180.0, epsilon = 1e-4);
    }

    #[test]
    fn deg2rad_zero_is_zero() {
        assert_eq!(deg2rad(0.0), 0.0);
    }

    #[test]
    fn euler_zero_is_identity() {
        let q = euler_xyz_to_quat(0.0, 0.0, 0.0);
        assert_relative_eq!(q.w, 1.0, epsilon = 1e-6);
        assert_relative_eq!(q.i, 0.0, epsilon = 1e-6);
        assert_relative_eq!(q.j, 0.0, epsilon = 1e-6);
        assert_relative_eq!(q.k, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn euler_90_roll_only() {
        let q = euler_xyz_to_quat(90.0, 0.0, 0.0);
        let s = (core::f32::consts::FRAC_PI_4).sin();
        let c = (core::f32::consts::FRAC_PI_4).cos();
        assert_relative_eq!(q.w, c, epsilon = 1e-6);
        assert_relative_eq!(q.i, s, epsilon = 1e-6);
        assert_relative_eq!(q.j, 0.0, epsilon = 1e-6);
        assert_relative_eq!(q.k, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn euler_xyz_roundtrip_zero() {
        let q = euler_xyz_to_quat(0.0, 0.0, 0.0);
        let e = quat_to_euler_xyz_deg(q);
        assert_relative_eq!(e.x, 0.0, epsilon = 1e-3);
        assert_relative_eq!(e.y, 0.0, epsilon = 1e-3);
        assert_relative_eq!(e.z, 0.0, epsilon = 1e-3);
    }

    #[test]
    fn euler_xyz_roundtrip_45() {
        let q = euler_xyz_to_quat(30.0, 45.0, 60.0);
        let e = quat_to_euler_xyz_deg(q);
        assert_relative_eq!(e.x, 30.0, epsilon = 1e-3);
        assert_relative_eq!(e.y, 45.0, epsilon = 1e-3);
        assert_relative_eq!(e.z, 60.0, epsilon = 1e-3);
    }

    #[test]
    fn yaw_only_extraction() {
        // SlimeIMU `GetYawFromQuaternion` extracts rotation about Y-axis directly (legacy
        // convention is Y-up despite `ToQuaternion` placing yaw on Z; reproduced here for
        // byte-parity). Feed a Y-axis quaternion.
        use core::f32::consts::FRAC_PI_4;
        let q = UnitQuaternion::from_axis_angle(&nalgebra::Vector3::y_axis(), FRAC_PI_4);
        assert_relative_eq!(yaw_deg_from_quat(q), 45.0, epsilon = 1e-3);
    }

    #[test]
    fn pitch_only_extraction() {
        // SlimeIMU `GetXAxisFromQuaternion` extracts X-axis rotation. euler_xyz_to_quat(30, 0, 0)
        // applies 30° about X (roll), which is what this helper measures.
        let q = euler_xyz_to_quat(30.0, 0.0, 0.0);
        assert_relative_eq!(pitch_deg_from_quat(q), 30.0, epsilon = 1e-3);
    }

    #[test]
    fn transform_v_identity_is_id() {
        let v = Vector3::new(1.0, 2.0, 3.0);
        let q = UnitQuaternion::identity();
        let out = transform_v(v, q);
        assert_relative_eq!(out, v, epsilon = 1e-6);
    }

    #[test]
    fn rotation_from_to_parallel_is_identity() {
        let from = Vector3::new(1.0, 0.0, 0.0);
        let to = Vector3::new(2.0, 0.0, 0.0);
        let q = rotation_from_to(from, to);
        assert_relative_eq!(q.w, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn rotation_from_to_x_to_y() {
        let from = Vector3::new(1.0, 0.0, 0.0);
        let to = Vector3::new(0.0, 1.0, 0.0);
        let q = rotation_from_to(from, to);
        assert_relative_eq!(q.w, core::f32::consts::FRAC_PI_4.cos(), epsilon = 1e-6);
        assert_relative_eq!(q.k, core::f32::consts::FRAC_PI_4.sin(), epsilon = 1e-6);
    }
}

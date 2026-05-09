use imu_math::coord::{euler_xyz_to_quat, quat_to_euler_xyz_deg};
use imu_math::quat::{axis_angle, normalize_or_identity};
use imu_math::{Quaternion, UnitQuaternion};
use proptest::prelude::*;

proptest! {
    #[test]
    fn normalize_yields_unit_norm(
        w in -10.0_f32..10.0,
        x in -10.0_f32..10.0,
        y in -10.0_f32..10.0,
        z in -10.0_f32..10.0,
    ) {
        let q = Quaternion::new(w, x, y, z);
        let u = normalize_or_identity(q);
        let mag = (u.w * u.w + u.i * u.i + u.j * u.j + u.k * u.k).sqrt();
        prop_assert!((mag - 1.0).abs() < 1e-5, "norm = {mag}");
    }

    #[test]
    fn axis_angle_roundtrip(
        roll in -89.0_f32..89.0,
        pitch in -89.0_f32..89.0,
        yaw in -179.0_f32..179.0,
    ) {
        let q = euler_xyz_to_quat(roll, pitch, yaw);
        let (axis, angle) = axis_angle(q);
        let q2 = UnitQuaternion::from_axis_angle(
            &nalgebra::Unit::new_normalize(axis),
            angle,
        );
        let dot = q.w * q2.w + q.i * q2.i + q.j * q2.j + q.k * q2.k;
        prop_assert!(dot.abs() > 0.999, "dot = {dot}");
    }

    #[test]
    fn euler_roundtrip_avoids_gimbal_lock(
        roll in -85.0_f32..85.0,
        pitch in -85.0_f32..85.0,
        yaw in -179.0_f32..179.0,
    ) {
        let q = euler_xyz_to_quat(roll, pitch, yaw);
        let e = quat_to_euler_xyz_deg(q);
        prop_assert!((e.x - roll).abs() < 0.5, "roll diff = {}", (e.x - roll).abs());
        prop_assert!((e.y - pitch).abs() < 0.5, "pitch diff = {}", (e.y - pitch).abs());
        prop_assert!((e.z - yaw).abs() < 0.5, "yaw diff = {}", (e.z - yaw).abs());
    }
}

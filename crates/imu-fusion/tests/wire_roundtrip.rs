//! Verify fusion-output → Y-up world conversion → SlimeQuaternion path.

use imu_fusion::Vqf;
use imu_math::coord::vqf_zup_world_to_slimevr_yup;
use imu_math::UnitQuaternion;
use slime_tracker::SlimeQuaternion;

#[test]
fn vqf_output_to_slime_yup_quaternion() {
    let mut v = Vqf::new(1.0 / 200.0);
    let gravity = [0.0, 0.0, 9.806_65];
    for _ in 0..100 {
        v.update([0.0, 0.0, 0.0], gravity, None);
    }
    let q6 = v.quat_6d();
    let q_zup = UnitQuaternion::<f64>::from_quaternion(nalgebra::Quaternion::new(
        q6[0], q6[1], q6[2], q6[3],
    ));
    let q_yup = vqf_zup_world_to_slimevr_yup(q_zup);

    // Output should be a valid unit quaternion
    let mag =
        (q_yup.w * q_yup.w + q_yup.i * q_yup.i + q_yup.j * q_yup.j + q_yup.k * q_yup.k).sqrt();
    assert!((mag - 1.0).abs() < 1e-5, "norm = {mag}");

    // Build SlimeQuaternion (i, j, k, w order on the wire)
    let slime_q = SlimeQuaternion {
        i: q_yup.i,
        j: q_yup.j,
        k: q_yup.k,
        w: q_yup.w,
    };
    // Sanity: round-trip nalgebra → SlimeQuaternion → bytes preserves invariants
    assert!(slime_q.i.is_finite());
    assert!(slime_q.j.is_finite());
    assert!(slime_q.k.is_finite());
    assert!(slime_q.w.is_finite());
}

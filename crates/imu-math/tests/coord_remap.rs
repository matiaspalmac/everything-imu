use approx::assert_relative_eq;
use imu_math::coord::{jsl_to_vqf_body, vqf_zup_world_to_slimevr_yup};
use imu_math::{UnitQuaternion, Vector3};

#[test]
fn jsl_remap_canonical() {
    let v = Vector3::new(1.0_f32, 2.0, 3.0);
    let out = jsl_to_vqf_body(v);
    assert_eq!(out, [1.0_f64, -3.0, 2.0]);
}

#[test]
fn jsl_remap_zero_is_zero() {
    let out = jsl_to_vqf_body(Vector3::zeros());
    assert_eq!(out, [0.0_f64, 0.0, 0.0]);
}

#[test]
fn world_remap_identity_zup_to_yup() {
    let q_zup = UnitQuaternion::<f64>::identity();
    let q_yup = vqf_zup_world_to_slimevr_yup(q_zup);
    let half = std::f32::consts::FRAC_PI_4;
    assert_relative_eq!(q_yup.w, half.cos(), epsilon = 1e-6);
    assert_relative_eq!(q_yup.i, -half.sin(), epsilon = 1e-6);
    assert_relative_eq!(q_yup.j, 0.0, epsilon = 1e-6);
    assert_relative_eq!(q_yup.k, 0.0, epsilon = 1e-6);
}

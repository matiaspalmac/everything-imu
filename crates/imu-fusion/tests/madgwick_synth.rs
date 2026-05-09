use imu_fusion::Madgwick;

/// First-frame analytical: gyro = 0, accel = (0, 0, 1) (face-up). With identity quat
/// and zero gyro, Madgwick reduces to gradient correction, which evaluates to s≈0
/// for this aligned input — quaternion remains at identity within float tolerance.
#[test]
fn first_frame_static_face_up_stays_near_identity() {
    let mut m = Madgwick::new(1.0 / 200.0);
    m.update_imu(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    let q = m.quaternion();
    let mag = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    assert!((mag - 1.0).abs() < 1e-5, "norm = {mag}");
    // Reference accel aligned with default identity → s vector zero → no correction.
    assert!((q[0] - 1.0).abs() < 1e-5, "w drifted: {}", q[0]);
}

#[test]
fn zero_accel_skip_preserves_state() {
    let mut m = Madgwick::new(1.0 / 200.0);
    m.update_imu(0.1, 0.2, 0.3, 0.0, 0.0, 0.0);
    assert_eq!(m.quaternion(), [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn update_marg_zero_mag_skips() {
    let mut m = Madgwick::new(1.0 / 200.0);
    m.update_marg(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0);
    assert_eq!(m.quaternion(), [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn update_marg_static_yields_normalized_quat() {
    let mut m = Madgwick::new(1.0 / 200.0);
    m.update_marg(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0);
    let q = m.quaternion();
    let mag = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    assert!((mag - 1.0).abs() < 1e-5);
}

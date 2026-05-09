//! Surface the JSL→VQF gravity sign question explicitly.
//!
//! JSL convention: gravity face-up reads (0, -1, 0) g. After the (X, -Z, Y) remap
//! that lands at (0, 0, -1) g → -9.80665 m/s² on Z. VQF expects measured specific
//! force (gravity reaction) which is +9.81 m/s² up at rest, so this test asserts
//! the negative-z outcome and FLAGS THE DISCREPANCY between legacy output and VQF
//! conventional input.
//!
//! Resolution if this test triggers a wire-parity failure later:
//!   (a) replicate the legacy bug for byte-parity, OR
//!   (b) negate accel z post-remap and document the divergence in DECISIONS.md.
//! Decision deferred until oracle / real-capture comparison runs.

use imu_math::coord::jsl_to_vqf_body;
use imu_math::Vector3;

const GRAVITY_M_S2: f32 = 9.806_65;

#[test]
fn jsl_face_up_gravity_post_remap_is_negative_z() {
    let accel_g_jsl = Vector3::new(0.0_f32, -1.0, 0.0);
    let accel_m_s2_jsl = accel_g_jsl * GRAVITY_M_S2;
    let post = jsl_to_vqf_body(accel_m_s2_jsl);
    assert!(
        (post[0] - 0.0).abs() < 1e-5,
        "x should be 0, got {}",
        post[0]
    );
    assert!(
        (post[1] - 0.0).abs() < 1e-5,
        "y should be 0, got {}",
        post[1]
    );
    assert!(
        (post[2] - (-(GRAVITY_M_S2 as f64))).abs() < 1e-4,
        "z should be -9.80665 (legacy convention); if VQF requires +9.81 \
         this test must be revisited and its resolution recorded in DECISIONS.md. \
         Got {}",
        post[2],
    );
}

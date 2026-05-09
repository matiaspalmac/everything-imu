use imu_math::coord::jsl_to_vqf_body;
use imu_math::Vector3;

/// Verify the JSL→VQF body remap matrix is a proper rotation (det = +1, no handedness flip).
#[test]
fn jsl_remap_preserves_handedness() {
    let col_x = jsl_to_vqf_body(Vector3::new(1.0, 0.0, 0.0));
    let col_y = jsl_to_vqf_body(Vector3::new(0.0, 1.0, 0.0));
    let col_z = jsl_to_vqf_body(Vector3::new(0.0, 0.0, 1.0));

    let det = col_x[0] * (col_y[1] * col_z[2] - col_y[2] * col_z[1])
        - col_y[0] * (col_x[1] * col_z[2] - col_x[2] * col_z[1])
        + col_z[0] * (col_x[1] * col_y[2] - col_x[2] * col_y[1]);

    assert!(
        (det - 1.0).abs() < 1e-9,
        "remap must be proper rotation, det = {det}"
    );
}

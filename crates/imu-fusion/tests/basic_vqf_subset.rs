use imu_fusion::{BasicVqf, Vqf, VqfParams};

#[test]
fn basic_vqf_matches_vqf_with_features_disabled() {
    let ts = 1.0 / 200.0;
    let mut basic = BasicVqf::new(ts);
    let mut full = Vqf::with_params(
        ts,
        VqfParams {
            motion_bias_est_enabled: false,
            rest_bias_est_enabled: false,
            mag_dist_rejection_enabled: false,
            ..VqfParams::default()
        },
    );

    let gyr = [0.05, 0.02, -0.01];
    let acc = [0.0, 0.0, 9.806_65];
    for _ in 0..100 {
        basic.update(gyr, acc);
        full.update(gyr, acc, None);
    }

    let b = basic.quat_6d();
    let f = full.quat_6d();
    for c in 0..4 {
        assert!(
            (b[c] - f[c]).abs() < 1e-12,
            "diverged at component {c}: {} vs {}",
            b[c],
            f[c]
        );
    }
}

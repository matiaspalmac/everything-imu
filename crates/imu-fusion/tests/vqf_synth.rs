use imu_fusion::Vqf;

#[test]
fn rest_detected_with_static_inputs() {
    let mut v = Vqf::new(1.0 / 200.0);
    let gravity = [0.0, 0.0, 9.806_65];
    // Feed 400 frames at 200 Hz = 2 s; rest_min_t default 1.5 s.
    for _ in 0..400 {
        v.update([0.0, 0.0, 0.0], gravity, None);
    }
    assert!(
        v.rest_detected(),
        "rest should be detected after 2 s of stillness"
    );
}

#[test]
fn bias_clip_clamps_excessive_input() {
    let mut v = Vqf::new(1.0 / 200.0);
    let gravity = [0.0, 0.0, 9.806_65];
    let big_gyro = [10.0, 10.0, 10.0]; // 10 rad/s ≫ bias_clip 2°/s
    for _ in 0..200 {
        v.update(big_gyro, gravity, None);
    }
    let (bias, _) = v.bias_estimate();
    let bias_clip = 2.0 * std::f64::consts::PI / 180.0;
    for &b in &bias {
        assert!(
            b.abs() <= bias_clip + 1e-9,
            "bias {b} exceeds clip {bias_clip}"
        );
    }
}

#[test]
fn set_bias_estimate_round_trip() {
    let mut v = Vqf::new(1.0 / 200.0);
    let target = [0.001, -0.002, 0.0005]; // small, within bias_clip
    v.set_bias_estimate(target, Some(0.01));
    let (got, _) = v.bias_estimate();
    for i in 0..3 {
        assert!((got[i] - target[i]).abs() < 1e-9);
    }
}

#[test]
#[should_panic(expected = "feed mag first")]
fn quat_9d_panics_without_mag() {
    let v = Vqf::new(1.0 / 200.0);
    let _ = v.quat_9d();
}

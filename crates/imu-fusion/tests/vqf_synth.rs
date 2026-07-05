use imu_fusion::Vqf;

/// Rotate a vector by a `[w, x, y, z]` quaternion (f64), so the tests do not
/// reach for the crate-private helper.
fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let [w, x, y, z] = q;
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

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

/// B1 regression: the inclination correction must run on the *normalized* 6D
/// earth-gravity direction. Feed a static, tilted gravity vector (non-zero
/// earth x/y, which the vertical-only oracle never exercised) and confirm the
/// 6D orientation rotates the measured gravity back onto earth vertical.
///
/// Analytic expectation: with `gyr = 0` and a constant accelerometer reading of
/// the gravity direction, VQF's accelerometer correction drives `acc_quat` to
/// the fixed point where `rotate(quat_6d, accel_unit) == [0, 0, 1]`. Independent
/// of the reference `vqf` package.
#[test]
fn tilted_static_gravity_aligns_to_vertical() {
    let mut v = Vqf::new(1.0 / 200.0);
    // 30° tilt about X: gravity in the body frame.
    let theta = 30.0_f64.to_radians();
    let g = 9.806_65;
    let accel = [0.0, -g * theta.sin(), g * theta.cos()];
    // 10 s of stillness — well past the accelerometer LP settling time.
    for _ in 0..2000 {
        v.update([0.0, 0.0, 0.0], accel, None);
    }

    let q = v.quat_6d();
    let n = (accel[0] * accel[0] + accel[1] * accel[1] + accel[2] * accel[2]).sqrt();
    let accel_unit = [accel[0] / n, accel[1] / n, accel[2] / n];
    let earth = rotate(q, accel_unit);
    assert!(
        earth[0].abs() < 0.02 && earth[1].abs() < 0.02 && (earth[2] - 1.0).abs() < 0.02,
        "tilted gravity should rotate to earth vertical, got {earth:?}"
    );
}

/// B2 regression: the motion-bias Kalman must be fed the normalized 6D gravity
/// direction, not the ~9.81 m/s² inertial-frame vector. With the old code the
/// residual was ~g/acc_ts too large and pinned the gyro-bias estimate against
/// its clip whenever the device was merely tilted. Here the true bias is zero
/// and the device is static but tilted; a correct filter keeps the estimate
/// near zero rather than driving it toward the clip.
///
/// Kept under the 1.5 s rest window (280 frames @ 200 Hz = 1.4 s) so the
/// measurement stays on the motion-bias branch the finding targets.
#[test]
fn tilted_static_motion_bias_stays_near_zero() {
    let mut v = Vqf::new(1.0 / 200.0);
    let theta = 40.0_f64.to_radians();
    let g = 9.806_65;
    let accel = [g * theta.sin(), 0.0, g * theta.cos()];
    for _ in 0..280 {
        v.update([0.0, 0.0, 0.0], accel, None);
    }
    let (bias, _) = v.bias_estimate();
    let half_clip = 1.0_f64.to_radians(); // half of the 2°/s bias clip
    for &b in &bias {
        assert!(
            b.abs() < half_clip,
            "motion-bias estimate {b} rad/s drifted toward the clip on a static tilt"
        );
    }
}

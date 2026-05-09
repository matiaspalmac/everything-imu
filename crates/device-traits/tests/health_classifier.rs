use device_traits::{DeviceHealth, HealthClassifier};
use std::time::{Duration, Instant};

#[test]
fn healthy_at_target_rate() {
    let mut h = HealthClassifier::new(60);
    let t0 = Instant::now();
    for i in 0..60 {
        h.record_sample(t0 + Duration::from_millis(i * 16));
    }
    let now = t0 + Duration::from_millis(60 * 16);
    assert_eq!(h.classify(now), DeviceHealth::Healthy);
}

#[test]
fn laggy_below_target() {
    let mut h = HealthClassifier::new(60);
    let t0 = Instant::now();
    for i in 0..10 {
        h.record_sample(t0 + Duration::from_millis(i * 100));
    }
    let now = t0 + Duration::from_millis(1000);
    assert_eq!(h.classify(now), DeviceHealth::LaggyImu);
}

#[test]
fn no_imu_after_silence() {
    let mut h = HealthClassifier::new(60);
    let t0 = Instant::now();
    h.record_sample(t0);
    let now = t0 + Duration::from_secs(3);
    assert_eq!(h.classify(now), DeviceHealth::NoImu);
}

#[test]
fn explicit_disconnect_dominates() {
    let mut h = HealthClassifier::new(60);
    let t0 = Instant::now();
    for i in 0..60 {
        h.record_sample(t0 + Duration::from_millis(i * 16));
    }
    h.record_disconnect();
    assert_eq!(
        h.classify(t0 + Duration::from_secs(1)),
        DeviceHealth::Disconnected
    );
}

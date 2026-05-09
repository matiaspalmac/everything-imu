use device_traits::{ButtonState, ResetButtonDetector, ResetKind};
use std::time::{Duration, Instant};

#[test]
fn short_press_yaw() {
    let mut d = ResetButtonDetector::new();
    let t0 = Instant::now();
    assert_eq!(
        d.observe(
            ButtonState::HomeOrCapture {
                home_pressed: true,
                capture_pressed: false
            },
            t0,
        ),
        None,
    );
    let release = t0 + Duration::from_millis(200);
    assert_eq!(
        d.observe(
            ButtonState::HomeOrCapture {
                home_pressed: false,
                capture_pressed: false
            },
            release,
        ),
        Some(ResetKind::Yaw),
    );
}

#[test]
fn long_press_full() {
    let mut d = ResetButtonDetector::new();
    let t0 = Instant::now();
    let _ = d.observe(
        ButtonState::HomeOrCapture {
            home_pressed: true,
            capture_pressed: false,
        },
        t0,
    );
    let release = t0 + Duration::from_millis(1500);
    assert_eq!(
        d.observe(
            ButtonState::HomeOrCapture {
                home_pressed: false,
                capture_pressed: false
            },
            release,
        ),
        Some(ResetKind::Full),
    );
}

#[test]
fn capture_only_emits_yaw_on_press() {
    let mut d = ResetButtonDetector::new();
    let t0 = Instant::now();
    let r = d.observe(ButtonState::CaptureOnly { pressed: true }, t0);
    assert_eq!(r, Some(ResetKind::Yaw));
}

#[test]
fn debounce_suppresses_double_trigger() {
    let mut d = ResetButtonDetector::new();
    let t0 = Instant::now();
    let _ = d.observe(
        ButtonState::HomeOrCapture {
            home_pressed: true,
            capture_pressed: false,
        },
        t0,
    );
    let _ = d.observe(
        ButtonState::HomeOrCapture {
            home_pressed: false,
            capture_pressed: false,
        },
        t0 + Duration::from_millis(150),
    );
    let r1 = d.observe(
        ButtonState::HomeOrCapture {
            home_pressed: true,
            capture_pressed: false,
        },
        t0 + Duration::from_millis(250),
    );
    assert_eq!(r1, None);
    let r2 = d.observe(
        ButtonState::HomeOrCapture {
            home_pressed: false,
            capture_pressed: false,
        },
        t0 + Duration::from_millis(280),
    );
    assert_eq!(r2, None);
}

#[test]
fn capture_button_on_pro_emits_yaw() {
    let mut d = ResetButtonDetector::new();
    let t0 = Instant::now();
    let r = d.observe(
        ButtonState::HomeOrCapture {
            home_pressed: false,
            capture_pressed: true,
        },
        t0,
    );
    assert_eq!(r, Some(ResetKind::Yaw));
}

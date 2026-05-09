//! Sliding-window IMU rate health classifier.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceHealth {
    Healthy,
    LaggyImu,
    NoImu,
    Disconnected,
}

/// Sliding-window IMU rate classifier.
///
/// Window length = 1 second. Healthy = ≥ 0.9 × target_hz samples in the last window.
/// Laggy = >0 samples but below threshold. NoImu = 0 samples in last `no_imu_after`.
/// Disconnected = explicit signal via [`HealthClassifier::record_disconnect`].
pub struct HealthClassifier {
    target_hz: u16,
    window: Duration,
    no_imu_after: Duration,
    samples: VecDeque<Instant>,
    last_seen: Option<Instant>,
    disconnected: bool,
}

impl HealthClassifier {
    pub fn new(target_hz: u16) -> Self {
        Self {
            target_hz,
            window: Duration::from_secs(1),
            no_imu_after: Duration::from_secs(2),
            samples: VecDeque::with_capacity(target_hz as usize * 2),
            last_seen: None,
            disconnected: false,
        }
    }

    pub fn record_sample(&mut self, now: Instant) {
        self.last_seen = Some(now);
        self.samples.push_back(now);
        self.disconnected = false;
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        while let Some(&t) = self.samples.front() {
            if t < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn record_disconnect(&mut self) {
        self.disconnected = true;
    }

    pub fn classify(&mut self, now: Instant) -> DeviceHealth {
        if self.disconnected {
            return DeviceHealth::Disconnected;
        }
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        while let Some(&t) = self.samples.front() {
            if t < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
        let count = self.samples.len() as f64;
        let target = self.target_hz as f64 * 0.9;
        if count >= target {
            return DeviceHealth::Healthy;
        }
        let last_age = self
            .last_seen
            .map(|t| now.saturating_duration_since(t))
            .unwrap_or(Duration::MAX);
        if count > 0.0 || last_age < self.no_imu_after {
            return DeviceHealth::LaggyImu;
        }
        DeviceHealth::NoImu
    }
}

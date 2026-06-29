//! Reset button detector with short-/long-press distinction.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetKind {
    Yaw,
    Full,
    Mounting,
}

/// Button input shape per controller variant.
#[derive(Debug, Clone, Copy)]
pub enum ButtonState {
    /// Generic short → Yaw, long → Full mapping (JC-R Home, Pro Home).
    HomeOrCapture {
        home_pressed: bool,
        capture_pressed: bool,
    },
    /// Capture-only path (JC-L) — always emits Yaw on press edge.
    CaptureOnly { pressed: bool },
}

pub struct ResetButtonDetector {
    pressed_since: Option<Instant>,
    capture_was_pressed: bool,
    long_threshold: Duration,
    debounce: Duration,
    last_emit: Option<Instant>,
}

impl ResetButtonDetector {
    pub fn new() -> Self {
        Self::with_thresholds(Duration::from_millis(1000), Duration::from_millis(300))
    }

    pub fn with_thresholds(long_threshold: Duration, debounce: Duration) -> Self {
        Self {
            pressed_since: None,
            capture_was_pressed: false,
            long_threshold,
            debounce,
            last_emit: None,
        }
    }

    pub fn observe(&mut self, button: ButtonState, now: Instant) -> Option<ResetKind> {
        let in_debounce = self
            .last_emit
            .map(|t| now.saturating_duration_since(t) < self.debounce)
            .unwrap_or(false);

        match button {
            ButtonState::HomeOrCapture {
                home_pressed,
                capture_pressed,
            } => {
                // Capture button: emit Yaw only on the press edge so a held
                // capture button fires exactly once, mirroring CaptureOnly.
                let capture_edge = capture_pressed && !self.capture_was_pressed;
                self.capture_was_pressed = capture_pressed;
                if capture_edge {
                    if in_debounce {
                        return None;
                    }
                    self.last_emit = Some(now);
                    return Some(ResetKind::Yaw);
                }

                match (home_pressed, self.pressed_since) {
                    (true, None) => {
                        self.pressed_since = Some(now);
                        None
                    }
                    (true, Some(_)) => None,
                    (false, Some(start)) => {
                        let held = now.saturating_duration_since(start);
                        self.pressed_since = None;
                        if in_debounce {
                            return None;
                        }
                        let kind = if held >= self.long_threshold {
                            ResetKind::Full
                        } else {
                            ResetKind::Yaw
                        };
                        self.last_emit = Some(now);
                        Some(kind)
                    }
                    (false, None) => None,
                }
            }
            ButtonState::CaptureOnly { pressed } => {
                if pressed && self.pressed_since.is_none() {
                    self.pressed_since = Some(now);
                    if in_debounce {
                        return None;
                    }
                    self.last_emit = Some(now);
                    Some(ResetKind::Yaw)
                } else if !pressed {
                    self.pressed_since = None;
                    None
                } else {
                    None
                }
            }
        }
    }
}

impl Default for ResetButtonDetector {
    fn default() -> Self {
        Self::new()
    }
}

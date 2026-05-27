//! User-facing wizard state machine for magnetometer hard-iron calibration.
//!
//! Wraps the lower-level [`crate::mag_cal::fit_sphere`] / [`crate::mag_cal::coverage`]
//! routines with a deterministic state machine the UI can drive: start a
//! capture, feed live mag samples while the user rotates the device, query
//! progress (coverage, sample count), and finalise into a [`MagCalibration`].
//!
//! Pure logic: no I/O, no async, no threading. The UI owns one instance per
//! in-progress device calibration and pumps samples in from the same place
//! the live fusion loop consumes them. Tests are deterministic — the
//! state machine never reads wall-clock time.
//!
//! Lifecycle:
//! ```text
//! Idle ──start()──► Collecting ──ingest(p) × N──► Collecting
//!                       │
//!                       └─finalize()─► Ready{calibration} | Failed{reason}
//!                                              │
//!                                              └─reset()──► Idle
//! ```

use crate::mag_cal::{calibrate, coverage as coverage_of, MagCalibration};
use serde::{Deserialize, Serialize};

/// Default cap on captured samples — bounds memory while leaving plenty for
/// a quality fit. Algebraic sphere fit is O(n) per ingest, so 4096 is cheap.
pub const DEFAULT_SAMPLE_CAP: usize = 4096;

/// Coverage threshold below which [`finalize`] refuses to produce a
/// calibration. 0.70 = 18 of 26 direction bins touched, enough for a
/// numerically stable hard-iron solve.
pub const MIN_ACCEPTABLE_COVERAGE: f32 = 0.70;

/// Maximum residual in the unit of input samples (typically µT) above which
/// the fit is treated as "user didn't rotate cleanly through the sphere" or
/// "heavy soft-iron distortion". Empirical; tune as we collect data.
pub const MAX_ACCEPTABLE_RESIDUAL: f32 = 8.0;

/// Reasons a finalize attempt may refuse to produce a calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FailureReason {
    /// Fewer than 12 samples — the algebraic fit needs more rows than
    /// unknowns to be meaningful.
    TooFewSamples,
    /// Coverage below [`MIN_ACCEPTABLE_COVERAGE`]. UI should prompt for
    /// more rotation.
    InsufficientCoverage,
    /// Algebraic solve failed (singular normal matrix, NaN, non-positive
    /// radius). Usually means coplanar samples.
    FitFailed,
    /// Fit succeeded but RMS residual exceeded [`MAX_ACCEPTABLE_RESIDUAL`].
    /// Often soft-iron distortion or moving through a magnetic field
    /// gradient — UI should suggest moving away from steel, electronics.
    ResidualTooLarge,
}

/// Wizard state at any moment.
#[derive(Debug, Clone, PartialEq)]
pub enum WizardState {
    Idle,
    Collecting { samples_taken: usize, coverage: f32 },
    Ready { calibration: MagCalibration },
    Failed { reason: FailureReason },
}

/// Snapshot suitable for UI rendering — `Copy`-able view of the wizard's
/// live state. The actual sample buffer stays inside the wizard.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WizardProgress {
    pub samples: u32,
    pub coverage: f32,
    pub min_coverage_target: f32,
}

pub struct MagCalWizard {
    samples: Vec<[f32; 3]>,
    cap: usize,
    state: WizardState,
    /// Cached running centre estimate (mean of samples) used for cheap
    /// coverage updates without re-running the full fit on every ingest.
    /// We re-fit only on `finalize` and at most once per `coverage_refresh_every`.
    coverage_refresh_every: usize,
    last_coverage_refresh: usize,
}

impl MagCalWizard {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_SAMPLE_CAP)
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            samples: Vec::new(),
            cap: cap.max(16),
            state: WizardState::Idle,
            coverage_refresh_every: 16,
            last_coverage_refresh: 0,
        }
    }

    pub fn state(&self) -> &WizardState {
        &self.state
    }

    /// Transition Idle → Collecting (no-op if already Collecting).
    /// Drops any previously captured samples.
    pub fn start(&mut self) {
        self.samples.clear();
        self.last_coverage_refresh = 0;
        self.state = WizardState::Collecting {
            samples_taken: 0,
            coverage: 0.0,
        };
    }

    /// Feed one raw magnetometer sample (any unit, but typically µT).
    /// Silently ignored unless the wizard is in `Collecting`.
    pub fn ingest(&mut self, sample: [f32; 3]) {
        if !matches!(self.state, WizardState::Collecting { .. }) {
            return;
        }
        if self.samples.len() >= self.cap {
            return;
        }
        // NaN guard — a bogus driver frame shouldn't pollute the fit.
        if sample.iter().any(|c| !c.is_finite()) {
            return;
        }
        self.samples.push(sample);
        self.maybe_refresh_progress();
    }

    fn maybe_refresh_progress(&mut self) {
        let n = self.samples.len();
        let should_refresh = n.saturating_sub(self.last_coverage_refresh) >= self.coverage_refresh_every
            || n < 32;
        if !should_refresh {
            if let WizardState::Collecting { samples_taken, .. } = &mut self.state {
                *samples_taken = n;
            }
            return;
        }
        self.last_coverage_refresh = n;
        let centre = mean_centre(&self.samples);
        let cov = coverage_of(&self.samples, centre);
        self.state = WizardState::Collecting {
            samples_taken: n,
            coverage: cov,
        };
    }

    /// Best-effort progress snapshot for UI rendering.
    pub fn progress(&self) -> WizardProgress {
        match &self.state {
            WizardState::Collecting {
                samples_taken,
                coverage,
            } => WizardProgress {
                samples: *samples_taken as u32,
                coverage: *coverage,
                min_coverage_target: MIN_ACCEPTABLE_COVERAGE,
            },
            _ => WizardProgress {
                samples: self.samples.len() as u32,
                coverage: 0.0,
                min_coverage_target: MIN_ACCEPTABLE_COVERAGE,
            },
        }
    }

    /// Attempt to finalise the calibration. Transitions the wizard into
    /// `Ready` on success or `Failed` on rejection; returns a reference to
    /// the new state for convenience.
    pub fn finalize(&mut self) -> &WizardState {
        if self.samples.len() < 12 {
            self.state = WizardState::Failed {
                reason: FailureReason::TooFewSamples,
            };
            return &self.state;
        }
        let Some(cal) = calibrate(&self.samples) else {
            self.state = WizardState::Failed {
                reason: FailureReason::FitFailed,
            };
            return &self.state;
        };
        if cal.coverage < MIN_ACCEPTABLE_COVERAGE {
            self.state = WizardState::Failed {
                reason: FailureReason::InsufficientCoverage,
            };
            return &self.state;
        }
        if cal.residual > MAX_ACCEPTABLE_RESIDUAL {
            self.state = WizardState::Failed {
                reason: FailureReason::ResidualTooLarge,
            };
            return &self.state;
        }
        self.state = WizardState::Ready { calibration: cal };
        &self.state
    }

    /// Back to Idle, drop all samples.
    pub fn reset(&mut self) {
        self.samples.clear();
        self.last_coverage_refresh = 0;
        self.state = WizardState::Idle;
    }

    /// Borrow the raw sample buffer for diagnostics (test asserts, debug
    /// export, etc.).
    pub fn samples(&self) -> &[[f32; 3]] {
        &self.samples
    }
}

impl Default for MagCalWizard {
    fn default() -> Self {
        Self::new()
    }
}

fn mean_centre(points: &[[f32; 3]]) -> [f32; 3] {
    if points.is_empty() {
        return [0.0; 3];
    }
    let mut sum = [0.0f64; 3];
    for p in points {
        sum[0] += p[0] as f64;
        sum[1] += p[1] as f64;
        sum[2] += p[2] as f64;
    }
    let n = points.len() as f64;
    [(sum[0] / n) as f32, (sum[1] / n) as f32, (sum[2] / n) as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate well-spread points on a sphere centred at `c` with radius `r`.
    fn sphere(c: [f32; 3], r: f32, steps: usize) -> Vec<[f32; 3]> {
        let mut pts = Vec::new();
        for i in 0..steps {
            let theta = std::f32::consts::PI * (i as f32 + 0.5) / steps as f32;
            for j in 0..steps {
                let phi = 2.0 * std::f32::consts::PI * j as f32 / steps as f32;
                pts.push([
                    c[0] + r * theta.sin() * phi.cos(),
                    c[1] + r * theta.sin() * phi.sin(),
                    c[2] + r * theta.cos(),
                ]);
            }
        }
        pts
    }

    #[test]
    fn default_state_is_idle() {
        let w = MagCalWizard::new();
        assert!(matches!(w.state(), WizardState::Idle));
    }

    #[test]
    fn ingest_outside_collecting_is_noop() {
        let mut w = MagCalWizard::new();
        w.ingest([1.0, 2.0, 3.0]);
        assert_eq!(w.samples().len(), 0);
    }

    #[test]
    fn start_then_ingest_accumulates_samples() {
        let mut w = MagCalWizard::new();
        w.start();
        for p in sphere([0.0; 3], 50.0, 6) {
            w.ingest(p);
        }
        assert_eq!(w.samples().len(), 36);
        match w.state() {
            WizardState::Collecting { samples_taken, .. } => assert_eq!(*samples_taken, 36),
            other => panic!("expected Collecting, got {other:?}"),
        }
    }

    #[test]
    fn finalize_too_few_samples_fails() {
        let mut w = MagCalWizard::new();
        w.start();
        for _ in 0..5 {
            w.ingest([1.0, 0.0, 0.0]);
        }
        let st = w.finalize().clone();
        assert!(matches!(
            st,
            WizardState::Failed {
                reason: FailureReason::TooFewSamples
            }
        ));
    }

    #[test]
    fn finalize_full_sphere_yields_ready_with_centre() {
        let mut w = MagCalWizard::new();
        w.start();
        let centre = [3.0_f32, -1.0, 2.5];
        for p in sphere(centre, 45.0, 10) {
            w.ingest(p);
        }
        let st = w.finalize().clone();
        let cal = match st {
            WizardState::Ready { calibration } => calibration,
            other => panic!("expected Ready, got {other:?}"),
        };
        for (k, (got, want)) in cal.offset.iter().zip(centre.iter()).enumerate() {
            assert!(
                (got - want).abs() < 0.5,
                "axis {k} got {got} want {want}",
            );
        }
        assert!(cal.coverage >= MIN_ACCEPTABLE_COVERAGE);
        assert!(cal.residual < MAX_ACCEPTABLE_RESIDUAL);
    }

    #[test]
    fn finalize_insufficient_coverage_fails() {
        // Sample only a thin band of the sphere — high sample count, low
        // direction coverage.
        let mut w = MagCalWizard::new();
        w.start();
        let centre = [0.0_f32; 3];
        let radius = 40.0;
        for i in 0..200 {
            let phi = 2.0 * std::f32::consts::PI * (i as f32 / 200.0);
            // Lock theta near the equator so all samples live in the same
            // narrow latitudinal band — coverage stays low.
            let theta = std::f32::consts::FRAC_PI_2;
            w.ingest([
                centre[0] + radius * theta.sin() * phi.cos(),
                centre[1] + radius * theta.sin() * phi.sin(),
                centre[2] + radius * theta.cos(),
            ]);
        }
        let st = w.finalize().clone();
        assert!(
            matches!(
                st,
                WizardState::Failed {
                    reason: FailureReason::InsufficientCoverage,
                },
            ),
            "expected insufficient-coverage, got {st:?}"
        );
    }

    #[test]
    fn reset_returns_to_idle_and_drops_samples() {
        let mut w = MagCalWizard::new();
        w.start();
        for p in sphere([0.0; 3], 50.0, 6) {
            w.ingest(p);
        }
        w.reset();
        assert!(matches!(w.state(), WizardState::Idle));
        assert_eq!(w.samples().len(), 0);
    }

    #[test]
    fn capacity_clamps_sample_buffer() {
        let mut w = MagCalWizard::with_capacity(32);
        w.start();
        for _ in 0..100 {
            w.ingest([1.0, 1.0, 1.0]);
        }
        assert_eq!(w.samples().len(), 32, "buffer must cap at requested capacity");
    }

    #[test]
    fn nan_samples_are_dropped() {
        let mut w = MagCalWizard::new();
        w.start();
        w.ingest([f32::NAN, 0.0, 0.0]);
        w.ingest([0.0, f32::INFINITY, 0.0]);
        w.ingest([1.0, 2.0, 3.0]);
        assert_eq!(w.samples().len(), 1);
    }

    #[test]
    fn progress_reports_target_threshold() {
        let mut w = MagCalWizard::new();
        w.start();
        let p = w.progress();
        assert_eq!(p.samples, 0);
        assert_eq!(p.min_coverage_target, MIN_ACCEPTABLE_COVERAGE);
    }

    #[test]
    fn re_starting_drops_prior_samples() {
        let mut w = MagCalWizard::new();
        w.start();
        for _ in 0..50 {
            w.ingest([1.0, 1.0, 1.0]);
        }
        w.start();
        assert_eq!(w.samples().len(), 0);
        assert!(matches!(w.state(), WizardState::Collecting { .. }));
    }
}

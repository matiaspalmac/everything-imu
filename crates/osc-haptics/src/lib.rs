//! VRChat OSC → device rumble bridge.
//!
//! VRChat has no native haptic API. Instead it broadcasts every avatar
//! parameter over OSC (UDP, default port 9001). Avatars built for haptics
//! carry `VRCContactReceiver` components that write those parameters when
//! touched. This crate listens for those parameters and converts them into
//! rumble commands for any everything-imu device with a motor.
//!
//! The bridge is a pure listener — no VRChat mod required.
//!
//! - [`config`] — the address→device mapping, persisted as JSON.
//! - [`mapping`] — pure OSC-value → intensity logic.
//! - [`listener`] — the async UDP runtime.

pub mod config;
pub mod listener;
pub mod mapping;
pub mod sniffer;
pub mod test_fire;

pub use config::{normalize_address, HapticConfig, HapticMode, HapticRule, DEFAULT_OSC_PORT};
pub use listener::{run_bridge, run_bridge_with_sniffer, RumbleSink, SnifferHandle};
pub use mapping::{resolve, HapticAction};
pub use sniffer::{Sniffer, SnifferEntry};
pub use test_fire::{simulate_dispatch, TestFireOutcome};

/// Sink the bridge drives. Implemented by the app's `AppState` so the bridge
/// stays decoupled from `core` — dependency inversion.
pub use listener::RumbleSink as Sink;

//! Tesla Fleet API → virtual IMU tracker bridge.
//!
//! Turns a Tesla vehicle's live heading + speed feed (vehicle Fleet API
//! streaming endpoint) into [`device_traits::ImuSample`] events that the
//! everything-imu pipeline forwards to SlimeVR-Server like any other tracker.
//!
//! Design summary:
//! - [`auth`] handles OAuth2 refresh-token rotation against
//!   `auth.tesla.com/oauth2/v3/token`.
//! - [`api`] wraps the REST endpoints we touch (`/api/1/vehicles`,
//!   `/api/1/vehicles/{id}/vehicle_data`, plus the streaming endpoint
//!   `wss://streaming.vn.teslamotors.com/streaming/`).
//! - [`imu`] synthesises gyro + accel samples from heading / speed deltas.
//! - [`device`] + [`factory`] expose [`device_traits::Device`] /
//!   [`device_traits::DeviceFactory`] impls so the existing supervisor
//!   discovers, starts, and tears down a Tesla "tracker" with no other
//!   wiring than a config struct in [`config`].
//!
//! Without an actual Tesla, [`synthetic::SyntheticTesla`] replays a recorded
//! drive (heading + speed time series) so the rest of the stack — fusion,
//! SlimeVR forwarding, UI — can be developed and tested end-to-end.

pub mod auth;
pub mod api;
pub mod config;
pub mod device;
pub mod factory;
pub mod imu;

pub mod synthetic;

pub use config::TeslaConfig;
pub use device::TeslaDevice;
pub use device_traits::{Device, DeviceFactory};
pub use factory::TeslaFactory;

//! PlayStation Vita device driver (forwarded homebrew over UDP).
//!
//! Like the 3DS, the Vita cannot be driven as a host peripheral, so a VitaSDK
//! homebrew reads the full 6-axis IMU via `sceMotionGetSensorState()` and streams
//! it over UDP. Unlike the 3DS, the Vita SDK already returns calibrated SI-ish
//! floats (accel in g, gyro in rad/s), so the wire carries `f32` values and no
//! raw-count scaling is needed on this side. See `docs/reference/vita_protocol.md`.

mod device;
mod factory;

pub use device::VitaPacket;
pub use factory::VitaFactory;

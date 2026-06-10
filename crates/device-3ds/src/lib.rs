//! Nintendo 3DS / 2DS device driver (forwarded homebrew over UDP).
//!
//! The console cannot be driven over USB/BT, so a homebrew app on the 3DS reads
//! its 6-axis IMU (`hidAccelRead` + `hidGyroRead`) and streams 12-byte packets
//! over UDP to this bridge — the same companion-forwarder shape as the Wii
//! Remote, but UDP and a full accel+gyro payload. See `docs/reference/3ds_protocol.md`.

mod device;
mod factory;

pub use device::ThreeDsPacket;
pub use factory::ThreeDsFactory;

//! eimu remote-hub ingest.
//!
//! A phone (everything-imu-mobile) forwards its own IMU and any BLE
//! controllers it owns over UDP using the eimu remote protocol v1. Each
//! announced `(hub ip, handle)` pair registers as one device in the
//! supervisor, so remote devices flow through the same fusion/mounting/
//! SlimeVR pipeline as locally connected hardware.

mod device;
mod factory;
pub mod protocol;

pub use device::RemoteDevice;
pub use factory::RemoteFactory;

//! Wii Remote (forwarded companion protocol) device driver.

mod device;
pub mod diagnostics;
mod factory;

pub use factory::WiiFactory;

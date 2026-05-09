//! Top-level error type for `core`.

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("slime-tracker: {0}")]
    Slime(String),
    #[error("device: {0}")]
    Device(#[from] device_traits::DeviceError),
    #[error("device disconnected")]
    DeviceDisconnected,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("supervisor: {0}")]
    Supervisor(String),
}

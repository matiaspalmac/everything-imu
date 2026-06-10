//! Top-level Tauri command error type.

use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize, specta::Type)]
#[serde(tag = "type", content = "message")]
pub enum IpcError {
    #[error("device not found")]
    NotFound,
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl From<everything_imu_core::AppError> for IpcError {
    fn from(e: everything_imu_core::AppError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<persistence::PersistenceError> for IpcError {
    fn from(e: persistence::PersistenceError) -> Self {
        Self::Internal(e.to_string())
    }
}

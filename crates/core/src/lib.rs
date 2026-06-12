//! Application orchestration: AppState owns the SlimeClient + per-device pipelines.

pub mod app_state;
pub mod error;
pub mod latency;
pub mod lazy_slime;
pub mod pipeline;
pub mod quat;
pub mod supervisor;

pub use app_state::AppState;
pub use error::AppError;
pub use pipeline::Pipeline;
pub use quat::QuatXyzw;
pub use supervisor::Supervisor;

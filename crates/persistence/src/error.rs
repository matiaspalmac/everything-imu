//! Error type for persistence layer.

#[derive(thiserror::Error, Debug)]
pub enum PersistenceError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid value for column {column}: {reason}")]
    InvalidValue {
        column: &'static str,
        reason: String,
    },
}

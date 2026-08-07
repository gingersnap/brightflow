//! Ingest error type.
//!
//! Kept distinct from `AppError` because ingest runs in background flush tasks as
//! well as HTTP handlers, and those have no response to turn an error into.

use thiserror::Error;

/// Ingest-specific error type.
#[derive(Debug, Error)]
pub enum IngestError {
    #[error("Database error: {0}")]
    Database(#[from] brightflow_store::SqliteError),

    #[error("Migration error: {0}")]
    Migration(#[from] brightflow_store::MigrateError),

    #[error("Polars error: {0}")]
    Polars(#[from] polars::error::PolarsError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Join error: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("{0}")]
    Other(String),
}

pub type IngestResult<T> = Result<T, IngestError>;

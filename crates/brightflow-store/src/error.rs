//! Error types for the store crate

use std::path::PathBuf;

/// Result type for store operations
pub type StoreResult<T> = Result<T, StoreError>;

/// Errors that can occur in store operations
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Table not found
    #[error("Table not found: {0}")]
    TableNotFound(String),

    /// Table already exists
    #[error("Table already exists: {0}")]
    TableAlreadyExists(String),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    /// Database error
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),

    /// Migration error
    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// Polars error
    #[error("Polars error: {0}")]
    Polars(#[from] polars::prelude::PolarsError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Optimistic concurrency check failed: the table changed underneath a
    /// full-rewrite (e.g. a sync merged while enrichment was materializing).
    #[error("Table '{table}' version conflict: expected {expected}, found {found}")]
    VersionConflict {
        table: String,
        expected: i64,
        found: i64,
    },

    /// Other error
    #[error("{0}")]
    Other(String),
}

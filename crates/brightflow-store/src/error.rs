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

    /// Invalid table name
    #[error("Invalid table name: {0}")]
    InvalidTableName(String),

    /// Schema mismatch during ingestion
    #[error("Schema mismatch: {0}")]
    SchemaMismatch(String),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    /// Manifest parse/serialize error
    #[error("Manifest error: {0}")]
    ManifestParse(String),

    /// Polars error
    #[error("Polars error: {0}")]
    Polars(#[from] polars::prelude::PolarsError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Other error
    #[error("{0}")]
    Other(String),
}

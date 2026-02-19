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

    /// Delta Lake error
    #[error("Delta Lake error: {0}")]
    DeltaLake(#[from] deltalake::DeltaTableError),

    /// Parquet error
    #[error("Parquet error: {0}")]
    Parquet(#[from] deltalake::parquet::errors::ParquetError),

    /// Arrow error
    #[error("Arrow error: {0}")]
    Arrow(#[from] deltalake::arrow::error::ArrowError),

    /// Polars error
    #[error("Polars error: {0}")]
    Polars(#[from] polars::prelude::PolarsError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// URL parse error
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    /// Delta kernel error
    #[error("Delta kernel error: {0}")]
    DeltaKernel(#[from] deltalake::kernel::Error),

    /// Other error
    #[error("{0}")]
    Other(String),
}

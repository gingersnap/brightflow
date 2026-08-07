//! The store's single error enum and its `StoreResult` alias. Infrastructure
//! failures (SQLite, migrations, Polars, IO) convert in via `#[from]`; the
//! remaining variants are domain signals callers can match on — notably
//! `VersionConflict`, which carries the expected/found versions of a failed
//! optimistic-concurrency check on a full-table rewrite.

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
    Db(#[from] crate::pool::SqliteError),

    /// Migration error
    #[error("Migration error: {0}")]
    Migration(#[from] crate::migrate::MigrateError),

    /// Legacy sqlx database error — carried only until the remaining crates
    /// leave sqlx; new store code must not construct it.
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// Legacy sqlx migration error, same transition status as `Sqlx`.
    #[error("Migration error: {0}")]
    SqlxMigration(#[from] sqlx::migrate::MigrateError),

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

impl StoreError {
    /// True when the underlying failure is a UNIQUE / PRIMARY KEY constraint
    /// violation — the store's stable signal for "this name already exists".
    /// Callers must use this instead of matching the driver error inside
    /// `Db`, so the driver stays a store-internal choice.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        match self {
            Self::Db(e) => e.is_unique_violation(),
            _ => false,
        }
    }
}

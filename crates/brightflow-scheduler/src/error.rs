//! The scheduler's error type.
//!
//! Deliberately narrow: the scheduler either talks to its SQLite database or it
//! doesn't, so everything else collapses into `Other`. A sync *failure* is not an
//! error here — it is recorded as a `SyncRun` row with a status, because a failed
//! sync is data the UI shows, not an exception the runner propagates.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("{0}")]
    Other(String),
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;

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

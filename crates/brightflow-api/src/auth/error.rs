//! The authentication error type.
//!
//! `InvalidCredentials` is deliberately one variant covering both "no such user"
//! and "wrong password" — distinguishing them in the type invites a handler that
//! distinguishes them in the response, which is the enumeration oracle
//! `auth::backend` goes out of its way to close.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("Password hashing error: {0}")]
    Password(String),

    #[error("User not found")]
    UserNotFound,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

pub type AuthResult<T> = Result<T, AuthError>;

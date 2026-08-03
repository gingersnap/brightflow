//! The authentication error type.
//!
//! A failed login must report `InvalidCredentials` whether the account was
//! missing or the password was wrong. The `UserNotFound` variant must not be
//! used to tell a caller which: any response that distinguishes them reopens the
//! account-enumeration oracle that `auth::backend` spends an extra Argon2
//! verification to close.

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

//! The authentication error type.
//!
//! There is deliberately no "user not found" variant. A failed login must report
//! `InvalidCredentials` whether the account was missing or the password was
//! wrong; any variant that distinguished them would invite a response that does
//! too, reopening the account-enumeration oracle that `auth::backend` spends an
//! extra Argon2 verification to close. Keep it that way.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("Password hashing error: {0}")]
    Password(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

pub type AuthResult<T> = Result<T, AuthError>;

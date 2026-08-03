//! Argon2 password hashing and verification.
//!
//! `Argon2::default()` is used rather than hand-tuned parameters so the cost
//! tracks the crate's current recommendation instead of a number that was right
//! once. Both functions are synchronous and CPU-bound by design — callers must
//! run them under `spawn_blocking` (see `auth::backend`).

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};

use crate::auth::error::{AuthError, AuthResult};

pub fn hash_password(plain: &str) -> AuthResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| AuthError::Password(e.to_string()))?;
    Ok(hash.to_string())
}

pub fn verify_password(plain: &str, hash: &str) -> AuthResult<bool> {
    let parsed = PasswordHash::new(hash).map_err(|e| AuthError::Password(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash).unwrap());
    }

    #[test]
    fn wrong_password_rejected() {
        let hash = hash_password("correct horse").unwrap();
        assert!(!verify_password("battery staple", &hash).unwrap());
    }

    #[test]
    fn invalid_hash_string_rejected() {
        // verify_password surfaces a parse error, not a panic
        assert!(verify_password("x", "not-a-valid-hash").is_err());
    }
}

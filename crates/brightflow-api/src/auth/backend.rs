//! `axum-login` authentication backend: email + Argon2 password verification.
//!
//! Two properties this module exists to hold, both easy to lose by writing the
//! obvious code:
//!
//! - **Argon2 never runs on an async worker.** Verification is deliberately
//!   expensive (~50-100ms of CPU); doing it inline pins a tokio worker thread for
//!   that whole time and starves every other request the thread was multiplexing.
//!   It runs under `spawn_blocking`.
//! - **Both failure paths cost the same.** Returning early on an unknown email
//!   skips the hash entirely, which makes a wrong-email response measurably
//!   faster than a wrong-password one — a remote oracle for "does this account
//!   exist". Unknown emails are verified against a fixed dummy hash instead, so
//!   the work is the same either way.

use std::sync::LazyLock;

use axum_login::{AuthnBackend, UserId};
use serde::Deserialize;

use crate::auth::db::AuthDb;
use crate::auth::error::AuthError;
use crate::auth::models::User;
use crate::auth::password::{hash_password, verify_password};

/// An Argon2 hash of a value no user can supply, verified against on the
/// unknown-email path so it costs the same as a real verification.
///
/// Built once at first use with the same parameters as a real password hash —
/// hardcoding a literal would silently stop matching if the Argon2 cost params
/// are ever tuned, quietly reopening the timing gap.
///
/// `expect` rather than a fallback literal: hashing a fixed string with a fresh
/// random salt has no failure mode short of a broken Argon2, and a hardcoded
/// fallback that turned out to be unparseable would make the unknown-email path
/// return 500 — a louder oracle than the one this closes.
#[expect(
    clippy::expect_used,
    reason = "the alternative is a hardcoded fallback hash, which if unparseable would \
              make the unknown-email path return 500 — a louder oracle than the timing \
              gap this closes. Hashing a fixed string with a fresh random salt has no \
              real failure mode; a panic here would be a broken Argon2, not bad input."
)]
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password("brightflow-timing-equalizer-not-a-password")
        .expect("argon2 hashing of a fixed string with a random salt cannot fail")
});

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Clone)]
pub struct AuthBackend {
    db: AuthDb,
}

impl AuthBackend {
    pub fn new(db: AuthDb) -> Self {
        Self { db }
    }
}

impl AuthnBackend for AuthBackend {
    type User = User;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let found = self.db.get_user_by_email(&creds.email).await?;

        // Verify against the real hash when the account exists and a fixed dummy
        // otherwise. Both branches do one Argon2 verification, so response time
        // doesn't reveal whether the email is registered.
        let hash = found
            .as_ref()
            .map_or_else(|| DUMMY_HASH.clone(), |u| u.password_hash.clone());

        let matched = tokio::task::spawn_blocking(move || verify_password(&creds.password, &hash))
            .await
            .map_err(|e| {
                AuthError::Password(format!("password verification task failed: {e}"))
            })??;

        // `found` is None on the unknown-email path, so this rejects regardless of
        // what the dummy verification returned.
        Ok(found.filter(|_| matched))
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        self.db.get_user_by_id(user_id).await
    }
}

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_hash_is_a_valid_argon2_hash() {
        // If this ever stops parsing, the unknown-email path would error out
        // instead of verifying — turning the timing equalizer back into an oracle,
        // and a louder one.
        assert!(verify_password("anything", &DUMMY_HASH).is_ok());
    }

    #[test]
    fn dummy_hash_does_not_match_its_own_plaintext_by_accident() {
        // Sanity: verification against the dummy returns false for a user-supplied
        // password, so a miss on the unknown-email path can't authenticate anyone.
        assert_eq!(verify_password("hunter2", &DUMMY_HASH).ok(), Some(false));
    }
}

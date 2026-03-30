use axum_login::{AuthnBackend, UserId};
use serde::Deserialize;

use crate::auth::db::AuthDb;
use crate::auth::error::AuthError;
use crate::auth::models::User;
use crate::auth::password::verify_password;

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
        let Some(found) = self.db.get_user_by_email(&creds.email).await? else {
            return Ok(None);
        };

        if verify_password(&creds.password, &found.password_hash)? {
            Ok(Some(found))
        } else {
            Ok(None)
        }
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        self.db.get_user_by_id(user_id).await
    }
}

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

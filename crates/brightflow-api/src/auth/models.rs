//! The `User` row and its `axum-login` identity implementation.
//!
//! `password_hash` is `#[serde(skip_serializing)]` and `#[ts(skip)]`: it must
//! never reach a client or the generated TypeScript. It doubles as the session
//! auth hash, which is what makes a password change invalidate existing sessions
//! for free.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub password_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

impl axum_login::AuthUser for User {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.id.clone()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

// Row mapping, fields matching columns by name.
brightflow_store::impl_from_row!(User {
    id,
    email,
    display_name,
    password_hash,
    created_at,
    updated_at,
});

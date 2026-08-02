//! HTTP handlers for the session-cookie auth surface: login, logout, and `me`.
//!
//! Auth posture: `login` is the only unauthenticated route here, and it is the
//! one place a client can make the server spend Argon2 CPU, so it is rate limited
//! per IP *before* authentication (see `auth::rate_limit`). `logout` and `me`
//! operate on the session `axum-login` already resolved.
//!
//! `UserResponse` exists rather than serializing `User` directly so the password
//! hash cannot leak through a future field addition — the wire shape is stated
//! explicitly here.

use crate::auth::{check_login, client_key, AuthSession, Credentials, User};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::shared::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
        }
    }
}

pub async fn login(
    State(state): State<AppState>,
    mut auth_session: AuthSession,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Json<UserResponse>, AppError> {
    // Checked before authenticating: the point is to cap how often a client can
    // make the server spend ~50-100ms of Argon2 CPU, so the throttle has to come
    // first. Rejections are logged — a throttled client is either a bug or an
    // attack, and both are worth seeing.
    let key = client_key(&headers);
    if !check_login(&state.login_limiter, &key, std::time::Instant::now()) {
        tracing::warn!("Login rate limit exceeded for {key}");
        return Err(AppError::TooManyRequests(
            "Too many login attempts. Please wait and try again.".to_string(),
        ));
    }

    let creds = Credentials {
        email: req.email,
        password: req.password,
    };

    let user = auth_session
        .authenticate(creds)
        .await
        .map_err(|e| AppError::Internal(format!("Authentication error: {e}")))?
        .ok_or(AppError::Unauthorized)?;

    auth_session
        .login(&user)
        .await
        .map_err(|e| AppError::Internal(format!("Session error: {e}")))?;

    Ok(Json(UserResponse::from(&user)))
}

pub async fn logout(mut auth_session: AuthSession) -> StatusCode {
    if auth_session.logout().await.is_err() {
        tracing::warn!("Failed to clear session on logout");
    }
    StatusCode::OK
}

pub async fn me(auth_session: AuthSession) -> Result<Json<UserResponse>, AppError> {
    let user = auth_session.user.ok_or(AppError::Unauthorized)?;
    Ok(Json(UserResponse::from(&user)))
}

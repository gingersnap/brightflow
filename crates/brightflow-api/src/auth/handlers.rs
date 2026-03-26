use crate::auth::{AuthSession, Credentials, User};
use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::shared::AppError;

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
    pub is_admin: bool,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            is_admin: user.is_admin,
        }
    }
}

pub async fn login(
    mut auth_session: AuthSession,
    Json(req): Json<LoginRequest>,
) -> Result<Json<UserResponse>, AppError> {
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

use crate::auth::{AuthSession, Credentials, User};
use axum::{extract::State, http::StatusCode, Json};
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

// ============================================================================
// Settings
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    pub data_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub data_mode: String,
}

pub async fn get_settings(
    State(state): State<AppState>,
    auth_session: AuthSession,
) -> Result<Json<SettingsResponse>, AppError> {
    let user = auth_session.user.ok_or(AppError::Unauthorized)?;

    let data_mode = if let Some(auth_db) = &state.auth_db {
        auth_db
            .get_data_mode(&user.id)
            .await
            .unwrap_or_else(|_| "memory".to_string())
    } else {
        "memory".to_string()
    };

    Ok(Json(SettingsResponse { data_mode }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    auth_session: AuthSession,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsResponse>, AppError> {
    let user = auth_session.user.ok_or(AppError::Unauthorized)?;

    // Validate data_mode
    let _mode: brightflow_core::DataMode = req
        .data_mode
        .parse()
        .map_err(|e: String| AppError::BadRequest(e))?;

    let auth_db = state
        .auth_db
        .as_ref()
        .ok_or_else(|| AppError::Internal("Auth database not configured".to_string()))?;

    auth_db
        .upsert_user_settings(&user.id, &req.data_mode)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to save settings: {e}")))?;

    Ok(Json(SettingsResponse {
        data_mode: req.data_mode,
    }))
}

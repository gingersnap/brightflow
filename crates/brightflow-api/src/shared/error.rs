use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Polars error: {0}")]
    Polars(#[from] polars::prelude::PolarsError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            AppError::InvalidQuery(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_QUERY", msg.clone())
            }
            AppError::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg.clone())
            }
            AppError::Polars(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "POLARS_ERROR",
                format!("Query execution failed: {e}"),
            ),
            AppError::Io(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "IO_ERROR",
                format!("File operation failed: {e}"),
            ),
            AppError::Json(e) => (
                StatusCode::BAD_REQUEST,
                "JSON_ERROR",
                format!("JSON parsing failed: {e}"),
            ),
            AppError::Join(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "TASK_ERROR",
                format!("Task execution failed: {e}"),
            ),
        };

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

impl AppError {
    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::BadRequest(_) => "BAD_REQUEST",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::InvalidQuery(_) => "INVALID_QUERY",
            AppError::Internal(_) => "INTERNAL_ERROR",
            AppError::Polars(_) => "POLARS_ERROR",
            AppError::Io(_) => "IO_ERROR",
            AppError::Json(_) => "JSON_ERROR",
            AppError::Join(_) => "TASK_ERROR",
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

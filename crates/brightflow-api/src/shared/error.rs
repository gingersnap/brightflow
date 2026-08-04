//! The API's error type and its HTTP representation.
//!
//! One enum for every failure mode so handlers can `?` freely, with a single
//! `IntoResponse` deciding status codes and a stable machine-readable `code` in
//! the body. Messages for internal failures are passed through, so variants that
//! wrap third-party errors should not carry anything secret.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Unauthorized")]
    Unauthorized,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    /// The server is running without a data store. The message text is a
    /// contract: the frontend matches on "No data store configured" to show
    /// its guided explanation.
    #[error("No data store configured")]
    StoreUnavailable,

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

    #[error("Store error: {0}")]
    Store(#[from] brightflow_store::StoreError),

    #[error("Analysis error: {0}")]
    Analysis(String),
}

impl AppError {
    /// The one place a variant maps to (status, machine code, message).
    /// `error_code()` and `IntoResponse` both read from here so the two can
    /// never disagree.
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Unauthorized".to_string(),
            ),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.clone()),
            Self::TooManyRequests(msg) => (
                StatusCode::TOO_MANY_REQUESTS,
                "TOO_MANY_REQUESTS",
                msg.clone(),
            ),
            Self::InvalidQuery(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_QUERY",
                msg.clone(),
            ),
            Self::StoreUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "STORE_UNAVAILABLE",
                "No data store configured".to_string(),
            ),
            Self::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg.clone(),
            ),
            Self::Polars(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "POLARS_ERROR",
                format!("Query execution failed: {e}"),
            ),
            Self::Io(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "IO_ERROR",
                format!("File operation failed: {e}"),
            ),
            Self::Json(e) => (
                StatusCode::BAD_REQUEST,
                "JSON_ERROR",
                format!("JSON parsing failed: {e}"),
            ),
            Self::Join(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "TASK_ERROR",
                format!("Task execution failed: {e}"),
            ),
            Self::Store(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "STORE_ERROR",
                format!("Store operation failed: {e}"),
            ),
            Self::Analysis(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "ANALYSIS_ERROR",
                msg.clone(),
            ),
        }
    }

    pub fn error_code(&self) -> &'static str {
        self.parts().1
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = self.parts();

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::Analysis(format!("Analysis failed: {err}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_errors_get_4xx_and_server_errors_5xx() {
        // Code and status come from the same match arm; this pins the
        // class of each variant so a new arm can't silently 500 a client
        // mistake (or vice versa).
        let cases: Vec<(AppError, StatusCode, &str)> = vec![
            (
                AppError::Unauthorized,
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
            ),
            (
                AppError::BadRequest("x".into()),
                StatusCode::BAD_REQUEST,
                "BAD_REQUEST",
            ),
            (
                AppError::NotFound("x".into()),
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
            ),
            (
                AppError::InvalidQuery("x".into()),
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_QUERY",
            ),
            (
                AppError::Conflict("x".into()),
                StatusCode::CONFLICT,
                "CONFLICT",
            ),
            (
                AppError::TooManyRequests("x".into()),
                StatusCode::TOO_MANY_REQUESTS,
                "TOO_MANY_REQUESTS",
            ),
            (
                AppError::StoreUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
                "STORE_UNAVAILABLE",
            ),
            (
                AppError::Internal("x".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
            ),
            (
                AppError::Analysis("x".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                "ANALYSIS_ERROR",
            ),
        ];
        for (err, status, code) in cases {
            let (s, c, _) = err.parts();
            assert_eq!(s, status, "{code}");
            assert_eq!(c, code);
            assert_eq!(err.error_code(), code);
        }
    }

    #[test]
    fn store_unavailable_message_is_the_frontend_contract() {
        // The frontend matches startsWith('No data store configured') to show
        // its guided explanation; changing this text breaks that match.
        let (_, _, msg) = AppError::StoreUnavailable.parts();
        assert_eq!(msg, "No data store configured");
    }
}

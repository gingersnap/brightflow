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

    #[error("Store error: {0}")]
    Store(#[from] brightflow_store::StoreError),

    #[error("Analysis error: {0}")]
    Analysis(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            Self::InvalidQuery(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_QUERY",
                msg.clone(),
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
                format!("Delta store operation failed: {e}"),
            ),
            Self::Analysis(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "ANALYSIS_ERROR",
                msg.clone(),
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
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::NotFound(_) => "NOT_FOUND",
            Self::InvalidQuery(_) => "INVALID_QUERY",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::Polars(_) => "POLARS_ERROR",
            Self::Io(_) => "IO_ERROR",
            Self::Json(_) => "JSON_ERROR",
            Self::Join(_) => "TASK_ERROR",
            Self::Store(_) => "STORE_ERROR",
            Self::Analysis(_) => "ANALYSIS_ERROR",
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::Analysis(format!("Analysis failed: {err}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;

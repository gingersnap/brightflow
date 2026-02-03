use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::analytics::handlers;
use crate::state::AppState;

/// Create the main application router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Health check
        .route("/health", get(health_check))
        // API routes
        .nest("/api", api_routes())
}

/// API routes under /api prefix
fn api_routes() -> Router<AppState> {
    Router::new()
        // Dataset management
        .route("/datasets", get(handlers::list_datasets))
        .route("/datasets/{id}", get(handlers::get_dataset))
        .route("/datasets/{id}", delete(handlers::delete_dataset))
        .route("/datasets/upload", post(handlers::upload_dataset))
        // Query execution
        .route("/query", post(handlers::execute_query))
        // WebSocket
        .route("/ws", get(handlers::ws_handler))
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

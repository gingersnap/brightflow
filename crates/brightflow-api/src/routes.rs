use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::analytics::handlers;
use crate::connect::handlers as connect_handlers;
use crate::insights::handlers as insights_handlers;
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
        // Available tables (metadata only, for lazy loading)
        .route("/tables", get(handlers::list_available_tables))
        .route("/tables/:name/load", post(handlers::load_table))
        // Dataset management (loaded datasets)
        .route("/datasets", get(handlers::list_datasets))
        .route("/datasets/{id}", get(handlers::get_dataset))
        .route("/datasets/{id}", delete(handlers::delete_dataset))
        .route("/datasets/upload", post(handlers::upload_dataset))
        // Query execution
        .route("/query", post(handlers::execute_query))
        // Insights
        .route("/insights/review", post(insights_handlers::run_review))
        .route("/insights/trends", post(insights_handlers::run_trends))
        // WebSocket
        .route("/ws", get(handlers::ws_handler))
        // Connectors
        .route("/connectors", get(connect_handlers::list_connectors))
        .route(
            "/connectors/:name/run",
            post(connect_handlers::run_connector),
        )
        .route("/connectors/runs", get(connect_handlers::list_runs))
        .route("/connectors/runs/:id", get(connect_handlers::get_run))
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

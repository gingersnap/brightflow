use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};

use crate::analytics::handlers;
use crate::auth::handlers as auth_handlers;
use crate::connect::handlers as connect_handlers;
use crate::insights::handlers as insights_handlers;
use crate::scheduler::handlers as scheduler_handlers;
use crate::state::AppState;

/// Create the main application router
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Health check (public)
        .route("/health", get(health_check))
        // API routes
        .nest("/api", api_routes())
}

/// API routes under /api prefix
fn api_routes() -> Router<AppState> {
    let public = Router::new()
        .route("/auth/login", post(auth_handlers::login))
        .route("/auth/logout", post(auth_handlers::logout))
        .route("/auth/me", get(auth_handlers::me));

    let protected = Router::new()
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
        // User settings
        .route(
            "/settings",
            get(auth_handlers::get_settings).put(auth_handlers::update_settings),
        )
        // Connectors (file-based config listing + run)
        .route("/connectors", get(connect_handlers::list_connectors))
        .route(
            "/connectors/unified",
            get(connect_handlers::list_unified_connectors),
        )
        .route(
            "/connectors/:name/run",
            post(connect_handlers::run_connector),
        )
        .route(
            "/connectors/:name/runs",
            get(connect_handlers::list_connector_runs),
        )
        .route(
            "/connectors/:name/schedule",
            post(connect_handlers::schedule_connector),
        )
        // Connector Config CRUD (DB-backed)
        .route(
            "/connector-configs",
            get(scheduler_handlers::list_connector_configs)
                .post(scheduler_handlers::create_connector_config),
        )
        .route(
            "/connector-configs/{id}",
            get(scheduler_handlers::get_connector_config)
                .put(scheduler_handlers::update_connector_config)
                .delete(scheduler_handlers::delete_connector_config),
        )
        // Scheduler Job CRUD
        .route(
            "/scheduler/jobs",
            get(scheduler_handlers::list_jobs).post(scheduler_handlers::create_job),
        )
        .route(
            "/scheduler/jobs/{id}",
            get(scheduler_handlers::get_job)
                .put(scheduler_handlers::update_job)
                .delete(scheduler_handlers::delete_job),
        )
        .route(
            "/scheduler/jobs/{id}/run",
            post(scheduler_handlers::trigger_run),
        )
        // Sync Runs & State
        .route("/sync/runs", get(scheduler_handlers::list_sync_runs))
        .route("/sync/runs/{id}", get(scheduler_handlers::get_sync_run))
        .route(
            "/sync/state/{connector_id}",
            get(scheduler_handlers::get_sync_state),
        )
        .route_layer(middleware::from_fn(require_auth));

    public.merge(protected)
}

/// Middleware that requires authentication
async fn require_auth(
    auth_session: brightflow_auth::AuthSession,
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    if auth_session.user.is_some() {
        next.run(request).await
    } else {
        crate::shared::AppError::Unauthorized.into_response()
    }
}

/// Use IntoResponse for the error
use axum::response::IntoResponse;

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

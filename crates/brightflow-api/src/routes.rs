use axum::{
    extract::State,
    middleware,
    routing::{delete, get, post, put},
    Json, Router,
};

use crate::analytics::handlers;
use crate::auth::handlers as auth_handlers;
use crate::connect::handlers as connect_handlers;
use crate::ingest::handlers as ingest_handlers;
use crate::insights::handlers as insights_handlers;
use crate::product_analytics::handlers as pa_handlers;
use crate::scheduler::handlers as scheduler_handlers;
use crate::state::AppState;
use crate::system::handlers as system_handlers;
use crate::web_analytics::handlers as wa_handlers;

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
        .route("/auth/me", get(auth_handlers::me))
        // Event ingestion (public — called by tracking script from external domains)
        .route("/event", post(ingest_handlers::ingest_event))
        .route("/collect", post(ingest_handlers::ingest_event))
        .route("/track", post(ingest_handlers::track_event))
        .route("/identify", post(ingest_handlers::identify_user))
        .route("/script.js", get(ingest_handlers::serve_script));

    let protected = Router::new()
        // Available tables (metadata only, for lazy loading)
        .route("/tables", get(handlers::list_available_tables))
        .route("/tables/{name}/load", post(handlers::load_table))
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
        // Connectors (DB-backed config + schedule + run)
        .route("/connectors", get(connect_handlers::list_connectors))
        .route(
            "/connectors/available",
            get(connect_handlers::list_available_connectors),
        )
        .route(
            "/connectors/unified",
            get(connect_handlers::list_unified_connectors),
        )
        .route(
            "/connectors/runs",
            get(connect_handlers::list_enriched_runs),
        )
        .route(
            "/connectors/{name}/run",
            post(connect_handlers::run_connector),
        )
        .route(
            "/connectors/{name}/runs",
            get(connect_handlers::list_connector_runs),
        )
        .route(
            "/connectors/{name}/schedule",
            post(connect_handlers::schedule_connector),
        )
        .route(
            "/connectors/{name}/token",
            put(connect_handlers::update_connector_token),
        )
        // Preset-based endpoints
        .route(
            "/presets/{id}/run",
            post(connect_handlers::run_preset),
        )
        .route(
            "/presets/{id}/schedule",
            post(connect_handlers::schedule_preset),
        )
        .route(
            "/schedules/{id}",
            delete(connect_handlers::delete_schedule),
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
        // System observability
        .route("/system/ws", get(system_handlers::system_ws_handler))
        // Source management
        .route(
            "/sources",
            get(ingest_handlers::list_sources).post(ingest_handlers::create_source),
        )
        .route(
            "/sources/{id}",
            get(ingest_handlers::get_source)
                .put(ingest_handlers::update_source)
                .delete(ingest_handlers::delete_source),
        )
        .route("/sources/{id}/snippet", get(ingest_handlers::get_snippet))
        // Web analytics dashboard queries
        .route("/analytics/{source_id}/stats", get(wa_handlers::stats))
        .route(
            "/analytics/{source_id}/timeseries",
            get(wa_handlers::timeseries),
        )
        .route(
            "/analytics/{source_id}/top-pages",
            get(wa_handlers::top_pages),
        )
        .route(
            "/analytics/{source_id}/referrers",
            get(wa_handlers::referrers),
        )
        .route("/analytics/{source_id}/utm", get(wa_handlers::utm))
        .route(
            "/analytics/{source_id}/devices",
            get(wa_handlers::devices),
        )
        .route("/analytics/{source_id}/geo", get(wa_handlers::geo))
        // Product analytics
        .route(
            "/analytics/{source_id}/events",
            get(pa_handlers::event_list),
        )
        .route(
            "/analytics/{source_id}/funnel",
            post(pa_handlers::funnel),
        )
        .route(
            "/analytics/{source_id}/retention",
            post(pa_handlers::retention),
        )
        .route(
            "/analytics/{source_id}/users",
            get(pa_handlers::search_users),
        )
        .route(
            "/analytics/{source_id}/users/{user_id}/timeline",
            get(pa_handlers::user_timeline),
        )
        .route(
            "/analytics/{source_id}/users/{user_id}/profile",
            get(pa_handlers::user_profile),
        )
        .route_layer(middleware::from_fn(require_auth));

    public.merge(protected)
}

/// Middleware that requires authentication
async fn require_auth(
    auth_session: crate::auth::AuthSession,
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
async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "uptime_secs": state.start_time.elapsed().as_secs(),
    }))
}

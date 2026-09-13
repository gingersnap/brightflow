//! The API surface: every route, in one place.
//!
//! This file is deliberately the index of the HTTP contract — there is no
//! hand-written API document, because one drifted from 95 routes down to 10
//! accurate ones. Reading this router is how you find out what the API does; the
//! typed request/response shapes are in `brightflow-app/src/types/generated/`.
//!
//! Routes are grouped by area and wrapped in an auth middleware layer, with the
//! public exceptions (event collection, the tracking script, health) mounted
//! outside it.

use axum::{
    extract::State,
    middleware,
    routing::{delete, get, post, put},
    Json, Router,
};

use crate::actions::handlers as actions_handlers;
use crate::agent::handlers as agent_handlers;
use crate::analytics::handlers;
use crate::auth::handlers as auth_handlers;
use crate::connect::handlers as connect_handlers;
use crate::enrichment::handlers as enrichment_handlers;
use crate::ingest::handlers as ingest_handlers;
use crate::insights::handlers as insights_handlers;
use crate::llm::handlers as llm_handlers;
use crate::product_analytics::handlers as pa_handlers;
use crate::scheduler::handlers as scheduler_handlers;
use crate::semantics::handlers as semantics_handlers;
use crate::sources::handlers as sources_handlers;
use crate::state::AppState;
use crate::system::handlers as system_handlers;
use crate::textexplore::handlers as textexplore_handlers;
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
        .route("/script.js", get(ingest_handlers::serve_script))
        // No public route takes a filesystem-bound path param today, but guard
        // the whole router anyway so that stays true by construction.
        .route_layer(middleware::from_fn(
            crate::shared::reject_unsafe_path_params,
        ));

    let protected = Router::new()
        // Available tables (metadata only, for lazy loading)
        .route("/tables", get(handlers::list_available_tables))
        .route(
            "/sources/{source_id}/tables/{name}/load",
            post(handlers::load_table),
        )
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
        .route("/insights/drivers", post(insights_handlers::run_drivers))
        .route(
            "/sources/{source_id}/tables/{table}/insights/history",
            get(insights_handlers::get_history).delete(insights_handlers::reset_history),
        )
        .route(
            "/sources/{source_id}/tables/{table}/insights/runs",
            get(insights_handlers::get_runs),
        )
        .route(
            "/sources/{source_id}/insights/latest",
            get(insights_handlers::get_latest_runs),
        )
        // Enrichment functions (versioned derived columns)
        .route(
            "/sources/{source_id}/tables/{table}/functions",
            get(enrichment_handlers::list_functions).post(enrichment_handlers::create_function),
        )
        .route(
            "/functions/{id}",
            get(enrichment_handlers::get_function)
                .put(enrichment_handlers::update_function)
                .delete(enrichment_handlers::delete_function),
        )
        .route(
            "/functions/{id}/versions",
            get(enrichment_handlers::list_versions),
        )
        .route(
            "/functions/{id}/sample-run",
            post(enrichment_handlers::sample_run),
        )
        .route(
            "/functions/{id}/estimate",
            get(enrichment_handlers::estimate),
        )
        .route("/functions/{id}/runs", post(enrichment_handlers::start_run))
        .route(
            "/functions/{id}/usage-by-language",
            get(crate::enrichment::health::usage_by_language),
        )
        .route(
            "/functions/{id}/materialize",
            post(enrichment_handlers::materialize_only),
        )
        .route(
            "/functions/{id}/reset",
            post(enrichment_handlers::reset_function),
        )
        .route(
            "/sources/{source_id}/tables/{table}/mentions/summary",
            get(crate::enrichment::mentions_api::mention_summary),
        )
        .route(
            "/sources/{source_id}/tables/{table}/unresolved-subjects",
            get(crate::enrichment::mentions_api::list_unresolved),
        )
        .route(
            "/sources/{source_id}/tables/{table}/unresolved-subjects/{id}",
            post(crate::enrichment::mentions_api::update_unresolved),
        )
        .route(
            "/sources/{source_id}/tables/{table}/vocabulary/import",
            post(crate::enrichment::mentions_api::import_vocabulary),
        )
        .route(
            "/sources/{source_id}/tables/{table}/vocabulary/health",
            get(crate::enrichment::health::vocabulary_health),
        )
        .route(
            "/sources/{source_id}/tables/{table}/tickets/summary",
            get(crate::enrichment::health::ticket_summary),
        )
        .route(
            "/enrichment/runs/{rid}",
            get(enrichment_handlers::get_run),
        )
        .route(
            "/enrichment/runs/{rid}/cancel",
            post(enrichment_handlers::cancel_run),
        )
        .route(
            "/actions",
            get(actions_handlers::feed).post(actions_handlers::dispatch),
        )
        .route("/actions/manifest", get(actions_handlers::manifest))
        .route(
            "/actions/pending-count",
            get(actions_handlers::pending_count),
        )
        .route("/actions/approve-all", post(actions_handlers::approve_all))
        .route("/actions/{id}/approve", post(actions_handlers::approve))
        .route("/actions/{id}/reject", post(actions_handlers::reject))
        .route("/actions/{id}/undo", post(actions_handlers::undo))
        .route(
            "/llm/providers",
            get(llm_handlers::list_providers).post(llm_handlers::upsert_provider),
        )
        .route(
            "/llm/providers/{id}",
            delete(llm_handlers::delete_provider),
        )
        .route("/llm/providers/{id}/test", post(llm_handlers::test_provider))
        .route(
            "/agent/runs",
            get(agent_handlers::list_runs).post(agent_handlers::start_run),
        )
        .route("/agent/runs/{id}", get(agent_handlers::get_run))
        .route("/agent/runs/{id}/cancel", post(agent_handlers::cancel_run))
        .route(
            "/agent/runs/{id}/undo-all",
            post(agent_handlers::undo_all),
        )
        // Column semantics & table settings (source-scoped)
        // Read-only: column_semantics writes go through the action bus
        // (Action::SetKpi / SetColumnPolarity) so they land in the action log.
        .route(
            "/sources/{source_id}/tables/{name}/semantics",
            get(semantics_handlers::list_semantics),
        )
        .route(
            "/sources/{source_id}/tables/{name}/semantics/changes",
            get(semantics_handlers::list_declaration_changes),
        )
        .route(
            "/sources/{source_id}/tables/{name}/settings",
            get(semantics_handlers::get_table_settings),
        )
        // The source as one Ossie document, generated from the resolved views.
        .route(
            "/sources/{source_id}/semantic-model",
            get(semantics_handlers::export_semantic_model),
        )
        // A pasted model becomes declared-layer rows under document:{name}.
        .route(
            "/sources/{source_id}/semantic-model/import",
            post(semantics_handlers::import_semantic_model),
        )
        // Text Explorer (no-LLM text filtering + words widget)
        .route(
            "/sources/{source_id}/tables/{table}/textexplore/search",
            post(textexplore_handlers::search),
        )
        // Vocabularies (read-only; writes go through POST /api/actions)
        .route(
            "/sources/{source_id}/tables/{table}/taxonomy",
            get(crate::enrichment::vocabulary_api::get_taxonomy),
        )
        // WebSocket
        .route("/ws", get(handlers::ws_handler))
        // Connectors (DB-backed config + schedule + run)
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
        // Scheduler job mutation
        .route(
            "/scheduler/jobs/{id}",
            put(scheduler_handlers::update_job).delete(scheduler_handlers::delete_job),
        )
        // System observability
        .route("/system/ws", get(system_handlers::system_ws_handler))
        // Unified sources (must be before /sources/{id})
        .route("/sources/unified", get(sources_handlers::list_unified_sources))
        // Persistent CSV uploads (must be before /sources/{id})
        .route("/sources/upload", post(handlers::upload_source))
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
        // Reject traversal in dynamic segments (source_id/table/name) before any
        // handler runs — these are joined onto filesystem paths here and in the
        // store. Layered alongside auth so it applies to every protected route.
        .route_layer(middleware::from_fn(
            crate::shared::reject_unsafe_path_params,
        ))
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

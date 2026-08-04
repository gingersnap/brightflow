//! The Brightflow HTTP API: axum router, shared state, and server bootstrap.
//!
//! `serve()` is the composition root: a fixed sequence of `bootstrap` phases
//! that resolve workspace paths, open the Parquet store and auth database,
//! seed and hydrate the caches, start the scheduler and ingestion loops, and
//! assemble the router. Startup deliberately fails loudly on a
//! misconfiguration it cannot make safe (see `bootstrap::build_cors`) rather
//! than degrading into a permissive default that nothing announces.
//!
//! Route definitions live in `routes.rs`, which is the index of the API surface;
//! per-area handlers live in their own modules.

// Allow certain pedantic lints that are too strict for API code:
// - cognitive_complexity: handler functions are naturally complex
// - too_many_lines: handler functions may be verbose
// - indexing_slicing: array access is bounds-checked at runtime
// - shadow_reuse: variable shadowing with related values is common in handlers
// - needless_pass_by_value: Axum extractors require owned values
#![allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::indexing_slicing,
    clippy::shadow_reuse,
    clippy::needless_pass_by_value
)]

mod bootstrap;

pub mod actions;
pub mod agent;
pub mod analytics;
pub mod auth;
pub mod connect;
pub mod enrichment;
pub mod ingest;
pub mod insights;
pub mod llm;
pub mod product_analytics;
pub mod routes;
pub mod scheduler;
pub mod semantics;
pub mod shared;
pub mod sources;
pub mod state;
pub mod system;
pub mod textexplore;
pub mod topics;
pub mod web_analytics;

use std::net::SocketAddr;
use std::sync::Arc;

use tower_http::trace::TraceLayer;

/// Configuration for the API server
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: std::net::Ipv4Addr,
    pub port: u16,
    pub default_dataset: Option<String>,
    /// Specific tables to load (if None, loads all)
    pub tables: Option<Vec<String>>,
    /// CORS origin (None = auto-detect based on APP_ENV)
    pub cors_origin: Option<String>,
    /// Workspace paths — single source of truth for all data locations
    pub paths: brightflow_core::WorkspacePaths,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: std::net::Ipv4Addr::LOCALHOST,
            port: 8080,
            default_dataset: None,
            tables: None,
            cors_origin: None,
            paths: brightflow_core::WorkspacePaths::from_env(),
        }
    }
}

impl ServeConfig {
    /// Create config from environment variables.
    ///
    /// All data paths are resolved by `WorkspacePaths` from `BRIGHTFLOW_DATA_DIR`.
    pub fn from_env() -> Self {
        let host = std::env::var("BRIGHTFLOW_API_HOST")
            .ok()
            .and_then(|h| h.parse().ok())
            .unwrap_or(std::net::Ipv4Addr::LOCALHOST);

        let port = std::env::var("BRIGHTFLOW_API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        let default_dataset = std::env::var("BRIGHTFLOW_DEFAULT_DATASET").ok();

        let tables = std::env::var("BRIGHTFLOW_TABLES")
            .ok()
            .map(|s| s.split(',').map(|t| t.trim().to_string()).collect());

        let cors_origin = std::env::var("BRIGHTFLOW_CORS_ORIGIN").ok();

        Self {
            host,
            port,
            default_dataset,
            tables,
            cors_origin,
            paths: brightflow_core::WorkspacePaths::from_env(),
        }
    }
}

/// Start the API server with the given configuration.
///
/// Accepts an optional `log_sender` from `init_tracing()` for broadcasting log events
/// to the system observability WebSocket. Pure orchestration: each phase lives in
/// `bootstrap`, in exactly this order.
pub async fn serve(
    config: ServeConfig,
    log_sender: Option<tokio::sync::broadcast::Sender<system::log_layer::LogEntry>>,
) -> anyhow::Result<()> {
    let paths = config.paths.clone();

    // Ensure all workspace directories exist
    paths.ensure_dirs()?;

    let mut state = bootstrap::build_state(&config).await?;

    // Replace log_sender if one was provided from init_tracing
    if let Some(sender) = log_sender {
        state.log_sender = sender;
    }

    // Spawn system metrics sampler
    let sampler_metrics = Arc::clone(&state.system_metrics);
    let sampler_start = state.start_time;
    tokio::spawn(system::sampler::run_sampler(sampler_metrics, sampler_start));

    // Store workspace paths on state for connector discovery
    state.paths = Some(paths.clone());

    bootstrap::seed_and_recover(&state).await;

    // Load column semantic overrides from SQLite
    state.load_overrides_from_store().await;

    let auth_db = bootstrap::init_auth(&mut state, &paths).await?;
    bootstrap::start_scheduler(&mut state, &paths).await?;
    bootstrap::start_ingest(&mut state, &paths).await;

    let cors = bootstrap::build_cors(
        config.cors_origin.as_deref(),
        std::env::var("APP_ENV").ok().as_deref(),
    )?;

    let (app, session_sweeper) = build_app(state, auth_db, cors).await?;

    // Bind and serve
    let addr = SocketAddr::from((config.host, config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| std::io::Error::new(e.kind(), format!("{e} (addr: {addr})")))?;

    tracing::info!("Brightflow API server running on http://{}", addr);

    axum::serve(listener, app).await?;

    session_sweeper.abort();

    Ok(())
}

/// Assemble the served app: router + trace + CORS + session/auth stack, in
/// the exact layer order production runs.
///
/// Public so integration tests can exercise route-layer contracts (the auth
/// wall, the path-param guard) as mounted, rather than re-assembling an
/// approximation that could drift.
///
/// Also returns the abort handle for the background expired-session sweeper;
/// `serve` aborts it on shutdown, tests can drop it.
pub async fn build_app(
    state: state::AppState,
    auth_db: auth::AuthDb,
    cors: tower_http::cors::CorsLayer,
) -> anyhow::Result<(axum::Router, tokio::task::AbortHandle)> {
    let (auth_layer, deletion_task) = bootstrap::build_session_auth_layers(auth_db).await?;
    let app = routes::create_router()
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(auth_layer)
        .with_state(state);
    Ok((app, deletion_task.abort_handle()))
}

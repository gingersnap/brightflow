//! The Brightflow HTTP API: axum router, shared state, and server bootstrap.
//!
//! `serve()` is the whole composition root — it resolves workspace paths, opens
//! the Parquet store and auth database, seeds and hydrates the caches, starts the
//! scheduler and ingestion loops, and assembles the router. Startup deliberately
//! fails loudly on a misconfiguration it cannot make safe (see the
//! `APP_ENV=production` CORS branch) rather than degrading into a permissive
//! default that nothing announces.
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
use std::time::Duration;

use axum::http::{self, HeaderValue};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tower_sessions::{cookie::SameSite, ExpiredDeletion, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

use crate::auth::{AuthBackend, AuthDb};

use crate::state::AppState;

/// Configuration for the API server
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: [u8; 4],
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
            host: [127, 0, 0, 1],
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
            .and_then(|h| {
                let parts: Vec<u8> = h.split('.').filter_map(|p| p.parse().ok()).collect();
                parts.try_into().ok()
            })
            .unwrap_or([127, 0, 0, 1]);

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
/// to the system observability WebSocket.
pub async fn serve(
    config: ServeConfig,
    log_sender: Option<tokio::sync::broadcast::Sender<system::log_layer::LogEntry>>,
) -> anyhow::Result<()> {
    let paths = &config.paths;

    // Ensure all workspace directories exist
    paths.ensure_dirs()?;

    let store_path = paths.store();
    let litehouse_url = paths.litehouse_url();

    // Initialize AppState with lazy loading - metadata only, no data loaded
    let has_store = store_path.exists() && store_path.is_dir();
    let mut state = match (&config.default_dataset, has_store) {
        // Both default dataset and store
        (Some(csv_path), true) if std::path::Path::new(csv_path).exists() => {
            tracing::info!("Loading default dataset from: {}", csv_path);
            tracing::info!(
                "Indexing tables from: {} (metadata only)",
                store_path.display()
            );
            let store = brightflow_store::ParquetStore::new(&store_path, &litehouse_url).await?;
            match AppState::with_default_and_store(csv_path, store).await {
                Ok(s) => {
                    if let Some(dataset) = s.datasets.get_dataset("default") {
                        tracing::info!(
                            "Default dataset loaded: {:?} rows, {:?} columns",
                            dataset.row_count(),
                            dataset.column_count()
                        );
                    }
                    s
                },
                Err(e) => {
                    tracing::warn!("Failed to initialize: {}, starting empty", e);
                    AppState::new()
                },
            }
        },
        // Only store - lazy load metadata only
        (_, true) => {
            tracing::info!(
                "Indexing tables from: {} (metadata only)",
                store_path.display()
            );
            let store = brightflow_store::ParquetStore::new(&store_path, &litehouse_url).await?;
            AppState::with_store(store).await
        },
        // Only default dataset
        (Some(csv_path), false) if std::path::Path::new(csv_path).exists() => {
            tracing::info!("Loading default dataset from: {}", csv_path);
            match AppState::with_default_dataset(csv_path).await {
                Ok(s) => {
                    if let Some(dataset) = s.datasets.get_dataset("default") {
                        tracing::info!(
                            "Default dataset loaded: {:?} rows, {:?} columns",
                            dataset.row_count(),
                            dataset.column_count()
                        );
                    }
                    s
                },
                Err(e) => {
                    tracing::warn!("Failed to load default dataset: {}, starting empty", e);
                    AppState::new()
                },
            }
        },
        (Some(path), false) => {
            tracing::warn!(
                "Default dataset not found at {}, starting with empty state",
                path
            );
            AppState::new()
        },
        // No data source configured
        (None, false) => AppState::new(),
    };

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

    // Seed column semantics from known schemas (one-time migration from TOML)
    if let Some(store) = state.store() {
        state::seed_column_semantics(store).await;
        state::hydrate_enrichment_overrides(&state, store).await;
        llm::seed_from_env(&state).await;
        // Any 'running' agent run from a previous process crashed mid-flight
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
        if let Ok(n) = store.db().fail_stuck_agent_runs(now).await {
            if n > 0 {
                tracing::warn!("Marked {n} stuck agent runs as failed");
            }
        }
        if let Ok(n) = store.db().fail_stuck_enrichment_runs().await {
            if n > 0 {
                tracing::warn!("Marked {n} stuck enrichment runs as failed");
            }
        }
    }

    // Load column semantic overrides from SQLite
    state.load_overrides_from_store().await;

    // Initialize auth database
    let auth_db_url = paths.auth_url();
    let auth_db = AuthDb::new(&auth_db_url).await?;
    let auth_db_arc = Arc::new(auth_db.clone());
    state.auth_db = Some(Arc::clone(&auth_db_arc));

    // Auto-seed admin user when database is empty
    if let (Ok(email), Ok(password)) = (
        std::env::var("BRIGHTFLOW_ADMIN_EMAIL"),
        std::env::var("BRIGHTFLOW_ADMIN_PASSWORD"),
    ) {
        if auth_db_arc.user_count().await.unwrap_or(1) == 0 {
            let hash = auth::hash_password(&password)
                .map_err(|e| anyhow::anyhow!("Failed to hash admin password: {e}"))?;
            auth_db_arc
                .create_user(&email, "Admin", &hash)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to seed admin user: {e}"))?;
            tracing::info!("Seeded admin user: {}", email);
        }
    }

    // Initialize scheduler if store is available
    if let Some(store) = state.store() {
        let scheduler_db_url = paths.scheduler_url();
        let scheduler_db = brightflow_scheduler::SchedulerDb::new(&scheduler_db_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize scheduler database: {e}"))?;
        let scheduler_db = Arc::new(scheduler_db);

        let scheduler = brightflow_scheduler::Scheduler::new(
            Arc::clone(&scheduler_db),
            Arc::clone(store),
            paths.clone(),
        );
        let scheduler = Arc::new(scheduler);
        state.scheduler = Some(Arc::clone(&scheduler));
        state.scheduler_db = Some(scheduler_db);

        // Post-sync hook: promoted llm_prompt functions run incrementally
        // after each endpoint merge, then insights auto-recompute (which
        // spawns and returns — the scheduler awaits this hook inline).
        let hook_state = state.clone();
        scheduler
            .set_post_sync_hook(Arc::new(move |source_id: String, table: String| {
                let sync_state = hook_state.clone();
                Box::pin(async move {
                    enrichment::post_sync(sync_state.clone(), source_id.clone(), table.clone())
                        .await;
                    insights::auto::post_sync(sync_state, source_id, table).await;
                })
            }))
            .await;

        // Start scheduler background loop
        tokio::spawn(async move {
            scheduler.start().await;
        });
        tracing::info!("Scheduler started with SQLite-backed jobs");
    }

    // Initialize event ingestion engine
    match ingest::init(&paths.ingest_url(), &paths.events_buffer(), paths.base()).await {
        Ok(ingest_state) => {
            let ingest_state = Arc::new(ingest_state);
            state.ingest = Some(Arc::clone(&ingest_state));

            // Start flush background task
            let flush_buffer = Arc::clone(&ingest_state.buffer);
            let flush_events_path = paths.events_store();
            let store_for_flush = state.store().map(Arc::clone);
            tokio::spawn(async move {
                let flush_task =
                    ingest::flush::FlushTask::new(flush_buffer, flush_events_path, store_for_flush);
                flush_task.start().await;
            });
            tracing::info!("Event ingestion engine started");
        },
        Err(e) => {
            tracing::warn!("Failed to initialize ingest engine: {e}");
        },
    }

    // Session store: SQLite with Moka in-memory cache
    let session_store = SqliteStore::new(auth_db.pool().clone());
    session_store.migrate().await?;

    let deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(Duration::from_hours(1)),
    );

    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("brightflow.sid")
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    let auth_backend = AuthBackend::new(auth_db);
    let auth_layer = axum_login::AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    // Configure CORS
    let cors = if let Some(ref origin) = config.cors_origin {
        CorsLayer::new()
            .allow_origin(
                origin
                    .parse::<HeaderValue>()
                    .unwrap_or_else(|_| HeaderValue::from_static("*")),
            )
            .allow_methods([
                http::Method::GET,
                http::Method::POST,
                http::Method::PUT,
                http::Method::DELETE,
                http::Method::OPTIONS,
            ])
            .allow_headers([
                http::header::CONTENT_TYPE,
                http::header::AUTHORIZATION,
                http::header::ACCEPT,
            ])
            .allow_credentials(true)
    } else if std::env::var("APP_ENV").as_deref() == Ok("production") {
        // Previously this fell through to CorsLayer::permissive() on the reasoning
        // that production sits behind a reverse proxy on the same origin, where CORS
        // is moot. But that assumption is invisible at runtime: if the proxy is ever
        // absent or misconfigured, the server comes up happily with a wildcard CORS
        // policy and nothing says so. Refusing to start makes the assumption explicit
        // at the moment it stops holding.
        //
        // (A permissive layer cannot carry credentials, so cookie auth would break
        // rather than leak — the failure mode was confusing, not catastrophic. Failing
        // at startup still beats failing mysteriously on the first cross-origin
        // request.)
        anyhow::bail!(
            "APP_ENV=production requires an explicit CORS origin. Set `cors_origin` in \
             the config (or BRIGHTFLOW_CORS_ORIGIN). If the API is served same-origin \
             behind a reverse proxy, set it to that origin."
        );
    } else {
        // Dev default
        CorsLayer::new()
            .allow_origin(
                "http://localhost:5173"
                    .parse::<HeaderValue>()
                    .unwrap_or_else(|_| HeaderValue::from_static("*")),
            )
            .allow_methods([
                http::Method::GET,
                http::Method::POST,
                http::Method::PUT,
                http::Method::DELETE,
                http::Method::OPTIONS,
            ])
            .allow_headers([
                http::header::CONTENT_TYPE,
                http::header::AUTHORIZATION,
                http::header::ACCEPT,
            ])
            .allow_credentials(true)
    };

    // Build router
    let app = routes::create_router()
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(auth_layer)
        .with_state(state);

    // Bind and serve
    let addr = SocketAddr::from((config.host, config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| std::io::Error::new(e.kind(), format!("{e} (addr: {addr})")))?;

    tracing::info!("Brightflow API server running on http://{}", addr);

    axum::serve(listener, app).await?;

    deletion_task.abort();

    Ok(())
}

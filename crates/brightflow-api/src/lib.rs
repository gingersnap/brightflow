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

pub mod analytics;
pub mod auth;
pub mod connect;
pub mod insights;
pub mod routes;
pub mod scheduler;
pub mod shared;
pub mod state;
pub mod system;

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
    /// Root data directory (derives workspace paths)
    pub data_dir: Option<String>,
    /// Path to Parquet store to auto-load tables from
    pub store_path: Option<String>,
    /// Specific tables to load (if None, loads all)
    pub tables: Option<Vec<String>>,
    /// Path to directory containing schema YAML files
    pub schema_dir: Option<String>,
    /// Path to directory containing connector config YAML files
    pub connector_config_dir: Option<String>,
    /// SQLite database URL for auth/sessions
    pub database_url: Option<String>,
    /// SQLite database URL for scheduler
    pub scheduler_database_url: Option<String>,
    /// SQLite database URL for Litehouse (store metadata)
    pub litehouse_database_url: Option<String>,
    /// CORS origin (None = auto-detect based on APP_ENV)
    pub cors_origin: Option<String>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: [127, 0, 0, 1],
            port: 8080,
            default_dataset: None,
            data_dir: None,
            store_path: None,
            tables: None,
            schema_dir: None,
            connector_config_dir: None,
            database_url: None,
            scheduler_database_url: None,
            litehouse_database_url: None,
            cors_origin: None,
        }
    }
}

impl ServeConfig {
    /// Create config from environment variables.
    ///
    /// If `BRIGHTFLOW_DATA_DIR` is set, workspace paths are derived from
    /// `{data_dir}/workspaces/default/`. Individual env vars still override.
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

        let data_dir = std::env::var("BRIGHTFLOW_DATA_DIR").ok();
        let ws = data_dir.as_ref().map(|d| format!("{d}/workspaces/default"));

        let default_dataset = std::env::var("BRIGHTFLOW_DEFAULT_DATASET").ok();

        let store_path = std::env::var("BRIGHTFLOW_STORE")
            .ok()
            .or_else(|| ws.as_ref().map(|w| format!("{w}/store")));

        let tables = std::env::var("BRIGHTFLOW_TABLES")
            .ok()
            .map(|s| s.split(',').map(|t| t.trim().to_string()).collect());

        let schema_dir = std::env::var("BRIGHTFLOW_SCHEMA_DIR")
            .ok()
            .or_else(|| ws.as_ref().map(|w| format!("{w}/schemas")));
        let connector_config_dir = std::env::var("BRIGHTFLOW_CONNECTOR_CONFIGS")
            .ok()
            .or_else(|| ws.as_ref().map(|w| format!("{w}/connector-configs")));
        let database_url = std::env::var("BRIGHTFLOW_DATABASE_URL")
            .ok()
            .or_else(|| ws.as_ref().map(|w| format!("sqlite:{w}/auth.db?mode=rwc")));
        let scheduler_database_url = std::env::var("BRIGHTFLOW_SCHEDULER_DATABASE_URL")
            .ok()
            .or_else(|| {
                ws.as_ref()
                    .map(|w| format!("sqlite:{w}/scheduler.db?mode=rwc"))
            });
        let litehouse_database_url = std::env::var("BRIGHTFLOW_LITEHOUSE_URL").ok().or_else(|| {
            ws.as_ref()
                .map(|w| format!("sqlite:{w}/litehouse.db?mode=rwc"))
        });
        let cors_origin = std::env::var("BRIGHTFLOW_CORS_ORIGIN").ok();

        Self {
            host,
            port,
            default_dataset,
            data_dir,
            store_path,
            tables,
            schema_dir,
            connector_config_dir,
            database_url,
            scheduler_database_url,
            litehouse_database_url,
            cors_origin,
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
    // Auto-create workspace directories if data_dir is set
    if let Some(ref data_dir) = config.data_dir {
        let ws_dir = format!("{data_dir}/workspaces/default");
        for sub in ["store", "schemas", "connector-configs"] {
            std::fs::create_dir_all(format!("{ws_dir}/{sub}")).ok();
        }
    }

    // Derive litehouse database URL from config or store path
    let litehouse_url_for_store = |store_path: &str| -> String {
        config
            .litehouse_database_url
            .clone()
            .unwrap_or_else(|| format!("sqlite:{store_path}/../litehouse.db?mode=rwc"))
    };

    // Initialize AppState with lazy loading - metadata only, no data loaded
    let mut state = match (&config.default_dataset, &config.store_path) {
        // Both default dataset and store
        (Some(csv_path), Some(store_path)) if std::path::Path::new(csv_path).exists() => {
            tracing::info!("Loading default dataset from: {}", csv_path);
            tracing::info!("Indexing tables from: {} (metadata only)", store_path);
            let litehouse_url = litehouse_url_for_store(store_path);
            let store = brightflow_store::ParquetStore::new(store_path, &litehouse_url).await?;
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
        (_, Some(store_path)) => {
            tracing::info!("Indexing tables from: {} (metadata only)", store_path);
            let litehouse_url = litehouse_url_for_store(store_path);
            let store = brightflow_store::ParquetStore::new(store_path, &litehouse_url).await?;
            AppState::with_store(store).await
        },
        // Only default dataset
        (Some(csv_path), None) if std::path::Path::new(csv_path).exists() => {
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
        (Some(path), None) => {
            tracing::warn!(
                "Default dataset not found at {}, starting with empty state",
                path
            );
            AppState::new()
        },
        // No data source configured
        (None, None) => AppState::new(),
    };

    // Replace log_sender if one was provided from init_tracing
    if let Some(sender) = log_sender {
        state.log_sender = sender;
    }

    // Spawn system metrics sampler
    let sampler_metrics = Arc::clone(&state.system_metrics);
    let sampler_start = state.start_time;
    tokio::spawn(system::sampler::run_sampler(sampler_metrics, sampler_start));

    // Load schema configs from YAML files
    if let Some(schema_dir) = &config.schema_dir {
        state.load_schemas_from_dir(std::path::Path::new(schema_dir));
    } else {
        // Default: look for schemas/ in the working directory
        state.load_schemas_from_dir(std::path::Path::new("schemas"));
    }

    // Set connector config directory
    if let Some(dir) = &config.connector_config_dir {
        state.connector_config_dir = Some(std::path::PathBuf::from(dir));
        tracing::info!("Connector configs directory: {}", dir);
    }

    // Initialize auth database
    let auth_db_url = config
        .database_url
        .clone()
        .unwrap_or_else(|| "sqlite:data/auth.db?mode=rwc".to_string());

    // Ensure data directory exists
    if let Some(path) = auth_db_url.strip_prefix("sqlite:") {
        let db_path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

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
                .create_user(&email, "Admin", &hash, true)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to seed admin user: {e}"))?;
            tracing::info!("Seeded admin user: {}", email);
        }
    }

    // Initialize scheduler if store is available
    if let Some(store) = state.store() {
        let scheduler_db_url = config
            .scheduler_database_url
            .clone()
            .unwrap_or_else(|| "sqlite:data/scheduler.db?mode=rwc".to_string());
        if let Some(path) = scheduler_db_url.strip_prefix("sqlite:") {
            let db_path = path.split('?').next().unwrap_or(path);
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                std::fs::create_dir_all(parent).ok();
            }
        }
        let scheduler_db = brightflow_scheduler::SchedulerDb::new(&scheduler_db_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize scheduler database: {e}"))?;
        let scheduler_db = Arc::new(scheduler_db);

        let scheduler =
            brightflow_scheduler::Scheduler::new(Arc::clone(&scheduler_db), Arc::clone(store));
        let scheduler = Arc::new(scheduler);
        state.scheduler = Some(Arc::clone(&scheduler));
        state.scheduler_db = Some(scheduler_db);

        // Start scheduler background loop
        tokio::spawn(async move {
            scheduler.start().await;
        });
        tracing::info!("Scheduler started with SQLite-backed jobs");
    }

    // Session store: SQLite with Moka in-memory cache
    let session_store = SqliteStore::new(auth_db.pool().clone());
    session_store.migrate().await?;

    let deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(Duration::from_secs(3600)),
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
        // Behind reverse proxy on same origin — CORS not needed
        CorsLayer::permissive()
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
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Brightflow API server running on http://{}", addr);

    axum::serve(listener, app).await?;

    deletion_task.abort();

    Ok(())
}

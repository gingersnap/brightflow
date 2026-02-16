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
pub mod insights;
pub mod routes;
pub mod shared;
pub mod state;

use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::state::AppState;

/// Configuration for the API server
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: [u8; 4],
    pub port: u16,
    pub default_dataset: Option<String>,
    /// Path to Delta Lake store to auto-load tables from
    pub delta_store_path: Option<String>,
    /// Specific Delta tables to load (if None, loads all)
    pub delta_tables: Option<Vec<String>>,
    /// Path to directory containing schema YAML files
    pub schema_dir: Option<String>,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: [127, 0, 0, 1],
            port: 8080,
            default_dataset: None,
            delta_store_path: None,
            delta_tables: None,
            schema_dir: None,
        }
    }
}

impl ServeConfig {
    /// Create config from environment variables
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

        let delta_store_path = std::env::var("BRIGHTFLOW_DELTA_STORE").ok();

        let delta_tables = std::env::var("BRIGHTFLOW_DELTA_TABLES")
            .ok()
            .map(|s| s.split(',').map(|t| t.trim().to_string()).collect());

        let schema_dir = std::env::var("BRIGHTFLOW_SCHEMA_DIR").ok();

        Self {
            host,
            port,
            default_dataset,
            delta_store_path,
            delta_tables,
            schema_dir,
        }
    }
}

/// Start the API server with the given configuration
pub async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    // Initialize AppState with lazy loading - metadata only, no data loaded
    let state = match (&config.default_dataset, &config.delta_store_path) {
        // Both default dataset and delta store
        (Some(csv_path), Some(store_path)) if std::path::Path::new(csv_path).exists() => {
            tracing::info!("Loading default dataset from: {}", csv_path);
            tracing::info!("Indexing Delta tables from: {} (metadata only)", store_path);
            let store = brightflow_store::DeltaStore::new(store_path);
            match AppState::with_default_and_delta_store(csv_path, store).await {
                Ok(s) => {
                    if let Some(dataset) = s.datasets.get_dataset("default") {
                        tracing::info!(
                            "Default dataset loaded: {} rows, {} columns",
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
        // Only delta store - lazy load metadata only
        (_, Some(store_path)) => {
            tracing::info!("Indexing Delta tables from: {} (metadata only)", store_path);
            let store = brightflow_store::DeltaStore::new(store_path);
            AppState::with_delta_store(store).await
        },
        // Only default dataset
        (Some(csv_path), None) if std::path::Path::new(csv_path).exists() => {
            tracing::info!("Loading default dataset from: {}", csv_path);
            match AppState::with_default_dataset(csv_path).await {
                Ok(s) => {
                    if let Some(dataset) = s.datasets.get_dataset("default") {
                        tracing::info!(
                            "Default dataset loaded: {} rows, {} columns",
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

    // Load schema configs from YAML files
    if let Some(schema_dir) = &config.schema_dir {
        state.load_schemas_from_dir(std::path::Path::new(schema_dir));
    } else {
        // Default: look for schemas/ in the working directory
        state.load_schemas_from_dir(std::path::Path::new("schemas"));
    }

    // Configure CORS (permissive for single-user tool)
    let cors = CorsLayer::very_permissive();

    // Build router
    let app = routes::create_router()
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    // Bind and serve
    let addr = SocketAddr::from((config.host, config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Brightflow API server running on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

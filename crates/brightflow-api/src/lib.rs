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
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: [127, 0, 0, 1],
            port: 8080,
            default_dataset: None,
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

        Self {
            host,
            port,
            default_dataset,
        }
    }
}

/// Start the API server with the given configuration
pub async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    // Initialize AppState
    let state = match &config.default_dataset {
        Some(path) if std::path::Path::new(path).exists() => {
            tracing::info!("Loading default dataset from: {}", path);
            match AppState::with_default_dataset(path).await {
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
        Some(path) => {
            tracing::warn!(
                "Default dataset not found at {}, starting with empty state",
                path
            );
            AppState::new()
        },
        None => AppState::new(),
    };

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

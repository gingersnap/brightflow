use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use brightflow_api::{routes, state::AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if present (before reading any env vars)
    dotenvy::dotenv().ok();

    // Initialize tracing/logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "brightflow_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Determine default dataset path
    let default_csv = std::env::var("BRIGHTFLOW_DEFAULT_DATASET")
        .unwrap_or_else(|_| "data/yc-companies.csv".to_string());

    // Initialize AppState with default dataset
    let state = if std::path::Path::new(&default_csv).exists() {
        tracing::info!("Loading default dataset from: {}", default_csv);
        match AppState::with_default_dataset(&default_csv).await {
            Ok(s) => {
                if let Some(dataset) = s.datasets.get_dataset("default") {
                    tracing::info!(
                        "Default dataset loaded: {} rows, {} columns",
                        dataset.row_count(),
                        dataset.column_count()
                    );
                }
                s
            }
            Err(e) => {
                tracing::warn!("Failed to load default dataset: {}, starting empty", e);
                AppState::new()
            }
        }
    } else {
        tracing::warn!(
            "Default dataset not found at {}, starting with empty state",
            default_csv
        );
        AppState::new()
    };

    // Configure CORS (permissive for single-user tool)
    let cors = CorsLayer::very_permissive();

    // Build router
    let app = routes::create_router()
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    // Bind and serve
    let host: [u8; 4] = std::env::var("BRIGHTFLOW_API_HOST")
        .ok()
        .and_then(|h| {
            let parts: Vec<u8> = h.split('.').filter_map(|p| p.parse().ok()).collect();
            parts.try_into().ok()
        })
        .unwrap_or([127, 0, 0, 1]);
    let port: u16 = std::env::var("BRIGHTFLOW_API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from((host, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Brightflow API server running on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

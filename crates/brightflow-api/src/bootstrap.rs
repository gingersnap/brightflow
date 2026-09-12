//! Server bootstrap phases, in the order `serve()` runs them.
//!
//! Each phase owns one concern (state construction, cache seeding, auth,
//! scheduler, ingestion, session/auth layers, CORS) so the composition root
//! stays a readable sequence. `build_cors` is pure — parameterized on the
//! app-env string — so the deliberate production bail (refuse to start
//! without an explicit origin rather than come up permissive) is unit-tested.

use std::sync::Arc;
use std::time::Duration;

use axum::http::{self, HeaderValue};
use tower_http::cors::CorsLayer;
use tower_sessions::{cookie::SameSite, ExpiredDeletion, Expiry, SessionManagerLayer};

use crate::auth::{AuthBackend, AuthDb, RusqliteSessionStore};
use crate::state::AppState;
use crate::ServeConfig;

/// The composed session + auth middleware stack.
pub(crate) type AuthLayer = axum_login::AuthManagerLayer<AuthBackend, RusqliteSessionStore>;

/// Handle of the expired-session sweeper, aborted on shutdown.
pub(crate) type SessionSweeper =
    tokio::task::JoinHandle<Result<(), tower_sessions::session_store::Error>>;

/// Build the initial `AppState` from the configured dataset/store sources.
pub(crate) async fn build_state(config: &ServeConfig) -> anyhow::Result<AppState> {
    let paths = &config.paths;
    let store_path = paths.store();
    let litehouse_url = paths.litehouse_url();

    // Initialize AppState with lazy loading - metadata only, no data loaded
    let has_store = store_path.exists() && store_path.is_dir();
    let state = match (&config.default_dataset, has_store) {
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
    Ok(state)
}

/// Seed store-backed caches and fail runs stranded by a previous crash.
pub(crate) async fn seed_and_recover(state: &AppState) {
    if let Some(store) = state.store() {
        crate::llm::seed_from_env(state).await;
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
}

/// Open the auth database, attach it to state, and auto-seed the admin user
/// when the database is empty and env credentials are provided.
pub(crate) async fn init_auth(
    state: &mut AppState,
    paths: &brightflow_core::WorkspacePaths,
) -> anyhow::Result<AuthDb> {
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
            let hash = crate::auth::hash_password(&password)
                .map_err(|e| anyhow::anyhow!("Failed to hash admin password: {e}"))?;
            auth_db_arc
                .create_user(&email, "Admin", &hash)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to seed admin user: {e}"))?;
            tracing::info!("Seeded admin user: {}", email);
        }
    }
    Ok(auth_db)
}

/// Start the scheduler loop and install the post-sync hook (store required).
pub(crate) async fn start_scheduler(
    state: &mut AppState,
    paths: &brightflow_core::WorkspacePaths,
) -> anyhow::Result<()> {
    let Some(store) = state.store().map(Arc::clone) else {
        return Ok(());
    };
    let scheduler_db_url = paths.scheduler_url();
    let scheduler_db = brightflow_scheduler::SchedulerDb::new(&scheduler_db_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize scheduler database: {e}"))?;
    let scheduler_db = Arc::new(scheduler_db);

    let scheduler =
        brightflow_scheduler::Scheduler::new(Arc::clone(&scheduler_db), store, paths.clone());
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
                crate::enrichment::post_sync(sync_state.clone(), source_id.clone(), table.clone())
                    .await;
                crate::insights::auto::post_sync(sync_state, source_id, table).await;
            })
        }))
        .await;

    // Start scheduler background loop
    tokio::spawn(async move {
        scheduler.start().await;
    });
    tracing::info!("Scheduler started with SQLite-backed jobs");
    Ok(())
}

/// Start the event-ingestion engine and its flush loop (best-effort: a
/// failure logs and the server runs without ingestion).
pub(crate) async fn start_ingest(state: &mut AppState, paths: &brightflow_core::WorkspacePaths) {
    match crate::ingest::init(&paths.ingest_url(), &paths.events_buffer(), paths.base()).await {
        Ok(ingest_state) => {
            let ingest_state = Arc::new(ingest_state);
            state.ingest = Some(Arc::clone(&ingest_state));

            // Start flush background task
            let flush_buffer = Arc::clone(&ingest_state.buffer);
            let flush_events_path = paths.events_store();
            let store_for_flush = state.store().map(Arc::clone);
            tokio::spawn(async move {
                let flush_task = crate::ingest::flush::FlushTask::new(
                    flush_buffer,
                    flush_events_path,
                    store_for_flush,
                );
                flush_task.start().await;
            });
            tracing::info!("Event ingestion engine started");
        },
        Err(e) => {
            tracing::warn!("Failed to initialize ingest engine: {e}");
        },
    }
}

/// Session store (SQLite) + auth middleware, plus the expired-session
/// deletion task whose handle the caller aborts on shutdown. Infallible and
/// sync since the session schema moved into the auth migrations: there is
/// nothing left here that can fail or wait.
pub(crate) fn build_session_auth_layers(auth_db: AuthDb) -> (AuthLayer, SessionSweeper) {
    // The tower_sessions schema is auth migration 006, applied when AuthDb
    // opened the database — the store itself never migrates anything.
    let session_store = RusqliteSessionStore::new(auth_db.pool().clone());

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
    (auth_layer, deletion_task)
}

/// CORS policy from the configured origin and `APP_ENV`, as a pure decision.
///
/// Explicit origin wins; `APP_ENV=production` without one refuses to start
/// (previously the server came up with a permissive wildcard policy and
/// nothing said so — see the error text); anything else gets the dev default.
pub(crate) fn build_cors(
    cors_origin: Option<&str>,
    app_env: Option<&str>,
) -> anyhow::Result<CorsLayer> {
    if let Some(origin) = cors_origin {
        return Ok(cors_for_origin(origin));
    }
    if app_env == Some("production") {
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
    }
    // Dev default
    Ok(cors_for_origin("http://localhost:5173"))
}

fn cors_for_origin(origin: &str) -> CorsLayer {
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
}

#[cfg(test)]
mod tests {
    use super::build_cors;

    #[test]
    fn explicit_origin_is_accepted_in_any_env() {
        assert!(build_cors(Some("https://app.example.com"), Some("production")).is_ok());
        assert!(build_cors(Some("https://app.example.com"), None).is_ok());
    }

    #[test]
    fn production_without_origin_refuses_to_start() {
        let err = build_cors(None, Some("production")).unwrap_err();
        assert!(err.to_string().contains("explicit CORS origin"));
    }

    #[test]
    fn dev_default_applies_otherwise() {
        assert!(build_cors(None, None).is_ok());
        assert!(build_cors(None, Some("development")).is_ok());
    }
}

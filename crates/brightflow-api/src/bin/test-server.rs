//! Minimal HTTP server for frontend→backend integration tests.
//!
//! Binds the real `build_app` router (real store + auth + ingest) to an
//! ephemeral port (default 0) and prints the bound address on stdout so a test
//! harness can discover it. Unlike `serve` it boots no scheduler, WebSocket,
//! log shipper, or background flush task — the integration tier only needs the
//! HTTP + storage path (see plans/2026-08-29_frontend-backend-integration-tests.md).
//!
//! The workspace resolves from env exactly as production does
//! (`BRIGHTFLOW_DATA_DIR` / `BRIGHTFLOW_WORKSPACE`), so the harness points it
//! at a fresh `brightflow_test_support` copy. `BRIGHTFLOW_TEST_HOST` /
//! `BRIGHTFLOW_TEST_PORT` override the bind (default 127.0.0.1:0).

use std::io::Write;
use std::net::Ipv4Addr;
use std::sync::Arc;

use anyhow::Context;
use brightflow_api::auth::AuthDb;
use brightflow_api::state::AppState;
use brightflow_core::WorkspacePaths;
use brightflow_store::ParquetStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let paths = WorkspacePaths::from_env();
    // Same first step production takes (`brightflow_api::serve`). Git stores no
    // empty directories, so a freshly cloned template arrives without the ones
    // that happen to be empty, and SQLite will not create a database under a
    // directory that does not exist. A harness that skipped this would fail
    // only on someone else's machine.
    paths
        .ensure_dirs()
        .context("create workspace directories")?;

    // Storage first: sources/auth both read/write the store, and source CRUD
    // lives behind the ingest engine.
    let store = ParquetStore::new(paths.store(), &paths.litehouse_url())
        .await
        .context("open store")?;
    let mut state = AppState::with_store(store).await;
    state.paths = Some(paths.clone());

    // `/api/sources` (list/create) require `state.ingest`; initialise just that
    // component — no scheduler, no flush loop — so source CRUD resolves while
    // background subsystems stay off.
    let ingest =
        brightflow_api::ingest::init(&paths.ingest_url(), &paths.events_buffer(), paths.base())
            .await
            .context("init ingest")?;
    state.ingest = Some(Arc::new(ingest));

    // Auth + session stack.
    let auth_db = AuthDb::new(&paths.auth_url()).await.context("open auth")?;
    state.auth_db = Some(Arc::new(auth_db.clone()));
    let (app, sweeper) =
        brightflow_api::build_app(state, auth_db, tower_http::cors::CorsLayer::new());

    let host = std::env::var("BRIGHTFLOW_TEST_HOST")
        .ok()
        .and_then(|h| h.parse::<Ipv4Addr>().ok())
        .unwrap_or(Ipv4Addr::LOCALHOST);
    let port = std::env::var("BRIGHTFLOW_TEST_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(0);

    let listener = tokio::net::TcpListener::bind((host, port))
        .await
        .with_context(|| format!("bind {host}:{port}"))?;
    let addr = listener.local_addr().context("bound address")?;
    // Machine-readable line the harness parses to discover the ephemeral port.
    // (`stdout().write_all` rather than `println!`, which the workspace lint
    // denies; flush so the spawned-test line reaches the harness promptly.)
    let mut out = std::io::stdout();
    writeln!(out, "TEST_SERVER_LISTENING {addr}")?;
    out.flush()?;

    // The sweeper is aborted by dropping its handle when the process exits:
    // `axum::serve` runs until the process is killed, so there is no path back
    // here to abort it explicitly. The harness kills this server; it has no
    // graceful-shutdown path and needs none.
    drop(sweeper);
    axum::serve(listener, app).await.context("serve")?;
    Ok(())
}

//! End-to-end: every recluster attempt lands in the action log.
//!
//! Recluster used to have a direct HTTP route that bypassed the action bus,
//! so refits never appeared in the log or the activity feed. That route is
//! gone; this test pins the closed state — dispatching `Action::Recluster`
//! writes an action-log row *even when the refit itself fails* (here: a bare
//! temp workspace with no embedding model), which is exactly the audit
//! property the bypass lacked.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use polars::prelude::*;

use brightflow_api::actions::types::{Action, ActionStatus, Scope};
use brightflow_api::actions::{dispatch_action, Actor};
use brightflow_api::state::AppState;
use brightflow_store::ParquetStore;

const SOURCE: &str = "test-source";
const TABLE: &str = "issues";

async fn state_with_planted_table(dir: &std::path::Path) -> AppState {
    let db_path = dir.join("meta.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let store = ParquetStore::new(dir.join("store"), &db_url).await.unwrap();

    let parquet_path = dir.join("issues.parquet");
    let mut df = df!(
        "id" => &[1_i64, 2, 3],
        "title" => &["a", "b", "c"],
        "body" => &["text one", "text two", "text three"],
    )
    .unwrap();
    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();
    store
        .ingest_parquet(SOURCE, TABLE, &parquet_path, None)
        .await
        .unwrap();

    AppState::with_store(store).await
}

#[tokio::test]
async fn dispatching_recluster_writes_an_action_log_row() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_planted_table(dir.path()).await;

    let response = dispatch_action(
        &state,
        Action::Recluster {
            scope: Scope {
                source_id: SOURCE.to_string(),
                table: TABLE.to_string(),
            },
            k: None,
            language: None,
            embedder: None,
            min_cluster_size: None,
            algorithm: None,
        },
        "req-recluster-1",
        Actor::Human,
    )
    .await
    .unwrap();

    // The refit fails in this bare workspace (no embedding model), but the
    // attempt is still logged — dispatch inserts the row before executing.
    assert!(matches!(
        response.status,
        ActionStatus::Applied | ActionStatus::Failed
    ));

    let rows = state.store().unwrap().db().list_actions(10).await.unwrap();
    let row = rows
        .iter()
        .find(|r| r.action_kind == "recluster")
        .expect("recluster action-log row must exist");
    assert_eq!(row.request_id, "req-recluster-1");
    assert!(row.resolved_at.is_some(), "attempt must be resolved");
}

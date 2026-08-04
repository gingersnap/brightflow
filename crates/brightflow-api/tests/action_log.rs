//! End-to-end pins on the action bus: logging and the undo round-trip.
//!
//! Recluster used to have a direct HTTP route that bypassed the action bus,
//! so refits never appeared in the log or the activity feed. That route is
//! gone; the first test pins the closed state — dispatching
//! `Action::Recluster` writes an action-log row *even when the refit itself
//! fails* (here: a bare temp workspace with no embedding model), which is
//! exactly the audit property the bypass lacked.
//!
//! The second test pins the full execute → capture-undo → undo round-trip
//! through the public surface, using `ExcludeTerm` because it needs no
//! embedding model. This is the seam the actions-module split must not cut.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests panic on failure by design"
)]

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

#[tokio::test]
async fn exclude_term_round_trips_through_undo() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with_planted_table(dir.path()).await;

    let response = dispatch_action(
        &state,
        Action::ExcludeTerm {
            scope: Scope {
                source_id: SOURCE.to_string(),
                table: TABLE.to_string(),
            },
            term: "Noise".to_string(),
        },
        "req-exclude-1",
        Actor::Human,
    )
    .await
    .unwrap();
    assert!(matches!(response.status, ActionStatus::Applied));

    let db = state.store().unwrap().db();
    let table_id = db
        .get_table(SOURCE, TABLE)
        .await
        .unwrap()
        .expect("planted table must exist")
        .id;
    let terms = db.get_excluded_terms(&table_id).await.unwrap();
    assert_eq!(
        terms.iter().map(|t| t.term.as_str()).collect::<Vec<_>>(),
        vec!["noise"],
        "term is stored lowercased"
    );

    let row = db
        .get_action(response.log_id)
        .await
        .unwrap()
        .expect("action-log row must exist");
    assert_eq!(row.status, "applied");
    assert!(row.undo_json.is_some(), "undoable action must capture undo");

    // Undo through the public HTTP-handler surface.
    let undone = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(response.log_id),
    )
    .await
    .unwrap();
    assert!(matches!(undone.0.status, ActionStatus::Undone));

    let undone_row = db.get_action(response.log_id).await.unwrap().unwrap();
    assert_eq!(undone_row.status, "undone");
    let terms_after = db.get_excluded_terms(&table_id).await.unwrap();
    assert!(terms_after.is_empty(), "undo must remove the excluded term");
}

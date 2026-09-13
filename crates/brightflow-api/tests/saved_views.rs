//! End-to-end pins on saved views through the action bus: save creates a
//! row and undo removes it, a second save with the view's id overwrites and
//! undo restores the previous spec, names are unique per table, rename and
//! delete round-trip through undo, and the source listing names the table.

#![expect(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "integration tests panic on failure by design and read top to bottom"
)]

use axum::extract::{Path, State};
use polars::prelude::*;
use serde_json::json;

use brightflow_api::actions::types::{Action, ActionStatus, Scope};
use brightflow_api::actions::{dispatch_action, Actor};
use brightflow_api::semantics::handlers::list_saved_views;
use brightflow_api::state::AppState;
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};

const SOURCE: &str = "test-source";
const TABLE: &str = "issues";

async fn state_with_planted_table(ws: &TestWorkspace) -> AppState {
    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();
    let parquet_path = ws.root().join("issues.parquet");
    let mut df = df!(
        "id" => &[1_i64, 2, 3],
        "title" => &["a", "b", "c"],
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

fn scope() -> Scope {
    Scope {
        source_id: SOURCE.to_string(),
        table: TABLE.to_string(),
    }
}

fn human() -> Actor {
    Actor::Human {
        user_id: "user-1".to_string(),
    }
}

async fn undo(state: &AppState, log_id: i64) {
    let undone = brightflow_api::actions::handlers::undo(State(state.clone()), Path(log_id))
        .await
        .unwrap()
        .0;
    assert_eq!(undone.status, ActionStatus::Undone);
}

#[tokio::test]
async fn saved_views_round_trip_through_the_bus_and_undo() {
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let store = state.store().unwrap();
    let views = || async {
        list_saved_views(State(state.clone()), Path(SOURCE.to_string()))
            .await
            .unwrap()
            .0
    };

    // Save creates a view with the person as its author.
    let spec_v1 = json!({ "version": 1, "query": { "limit": 50 } });
    let saved = dispatch_action(
        &state,
        Action::SaveView {
            scope: scope(),
            name: "  Open bugs ".to_string(),
            spec: spec_v1.clone(),
            view_id: None,
        },
        "req-save-1",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(saved.status, ActionStatus::Applied);
    let view_id = saved.result["view_id"].as_str().unwrap().to_string();
    let listed = views().await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Open bugs");
    assert_eq!(listed[0].table, TABLE);
    assert_eq!(listed[0].spec, spec_v1);
    assert_eq!(listed[0].created_by.as_deref(), Some("user:user-1"));

    // The same name again on the same table is refused; a blank name too.
    let duplicate = dispatch_action(
        &state,
        Action::SaveView {
            scope: scope(),
            name: "Open bugs".to_string(),
            spec: json!({}),
            view_id: None,
        },
        "req-save-dup",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(duplicate.status, ActionStatus::Failed);
    let blank = dispatch_action(
        &state,
        Action::SaveView {
            scope: scope(),
            name: "   ".to_string(),
            spec: json!({}),
            view_id: None,
        },
        "req-save-blank",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(blank.status, ActionStatus::Failed);

    // Overwriting keeps the id and changes name and spec; undo restores both.
    let spec_v2 = json!({ "version": 1, "query": { "limit": 10 } });
    let overwritten = dispatch_action(
        &state,
        Action::SaveView {
            scope: scope(),
            name: "Open bugs, top 10".to_string(),
            spec: spec_v2.clone(),
            view_id: Some(view_id.clone()),
        },
        "req-save-2",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(overwritten.status, ActionStatus::Applied);
    assert_eq!(overwritten.result["created"], false);
    let after = views().await;
    assert_eq!(after[0].id, view_id);
    assert_eq!(after[0].name, "Open bugs, top 10");
    assert_eq!(after[0].spec, spec_v2);
    undo(&state, overwritten.log_id).await;
    let restored = views().await;
    assert_eq!(restored[0].name, "Open bugs");
    assert_eq!(restored[0].spec, spec_v1);

    // Rename and its undo.
    let renamed = dispatch_action(
        &state,
        Action::RenameView {
            scope: scope(),
            view_id: view_id.clone(),
            name: "Bugs".to_string(),
        },
        "req-rename",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(renamed.status, ActionStatus::Applied);
    assert_eq!(views().await[0].name, "Bugs");
    undo(&state, renamed.log_id).await;
    assert_eq!(views().await[0].name, "Open bugs");

    // Delete removes it; undo puts the same row back.
    let deleted = dispatch_action(
        &state,
        Action::DeleteView {
            scope: scope(),
            view_id: view_id.clone(),
        },
        "req-delete",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(deleted.status, ActionStatus::Applied);
    assert!(views().await.is_empty());
    undo(&state, deleted.log_id).await;
    let back = views().await;
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].id, view_id);
    assert_eq!(back[0].spec, spec_v1);

    // Undoing the original save removes the view for good.
    undo(&state, saved.log_id).await;
    assert!(views().await.is_empty());
    assert!(store.db().get_saved_view(&view_id).await.unwrap().is_none());
}

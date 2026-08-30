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
    clippy::too_many_lines,
    reason = "integration tests panic on failure by design and read top to bottom"
)]

use polars::prelude::*;

use brightflow_api::actions::types::{Action, ActionStatus, Scope};
use brightflow_api::actions::{dispatch_action, Actor};
use brightflow_api::state::AppState;
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};

const SOURCE: &str = "test-source";
const TABLE: &str = "issues";

/// Boot the store from a copy of the committed test workspace, then plant the
/// fixture `issues` table on top of it (a separate source from the template's
/// committed table). The store still resolves against a genuinely migrated
/// Litehouse and a real copy of the committed Parquet tree — the same genuine
/// read path production boots — before this test's own writes land.
async fn state_with_planted_table(ws: &TestWorkspace) -> AppState {
    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();

    let parquet_path = ws.root().join("issues.parquet");
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
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;

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
        Actor::Human {
            user_id: "user-1".to_string(),
        },
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
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;

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
        Actor::Human {
            user_id: "user-1".to_string(),
        },
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

/// The vocabulary hierarchy through the public bus: placement rules, the cap,
/// the frozen guard, and the audit line naming the human who acted.
#[tokio::test]
async fn vocabulary_actions_enforce_hierarchy_and_log_the_user() {
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let scope = || Scope {
        source_id: SOURCE.to_string(),
        table: TABLE.to_string(),
    };
    let jens = || Actor::Human {
        user_id: "user-jens".to_string(),
    };

    // A root category.
    let billing = dispatch_action(
        &state,
        Action::DefineTaxonomyCategory {
            scope: scope(),
            name: "billing".to_string(),
            description: Some("charges and invoices".to_string()),
            vocab_kind: None,
            parent_id: None,
            aliases: None,
        },
        "req-vocab-1",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(billing.status, ActionStatus::Applied));
    let billing_id = billing.result["categoryId"].as_i64().unwrap();

    // A subcategory needs a parent...
    let err = dispatch_action(
        &state,
        Action::DefineTaxonomyCategory {
            scope: scope(),
            name: "vat".to_string(),
            description: None,
            vocab_kind: Some("subcategory".to_string()),
            parent_id: None,
            aliases: None,
        },
        "req-vocab-2",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(err.status, ActionStatus::Failed), "{err:?}");

    // ...and lands under one.
    let vat = dispatch_action(
        &state,
        Action::DefineTaxonomyCategory {
            scope: scope(),
            name: "vat".to_string(),
            description: None,
            vocab_kind: Some("subcategory".to_string()),
            parent_id: Some(billing_id),
            aliases: None,
        },
        "req-vocab-3",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(vat.status, ActionStatus::Applied), "{vat:?}");
    assert_eq!(vat.result["parentId"], billing_id);

    // The reserved value is never a row.
    let other = dispatch_action(
        &state,
        Action::DefineTaxonomyCategory {
            scope: scope(),
            name: "Other".to_string(),
            description: None,
            vocab_kind: None,
            parent_id: None,
            aliases: None,
        },
        "req-vocab-4",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(other.status, ActionStatus::Failed));

    // Frozen refuses rename; unfreeze via undo-free path still logs.
    let frozen = dispatch_action(
        &state,
        Action::FreezeTaxonomyCategory {
            scope: scope(),
            category_id: billing_id,
            frozen: true,
        },
        "req-vocab-5",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(frozen.status, ActionStatus::Applied));
    let rename = dispatch_action(
        &state,
        Action::RenameTaxonomyCategory {
            scope: scope(),
            category_id: billing_id,
            name: "payments".to_string(),
        },
        "req-vocab-6",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(rename.status, ActionStatus::Failed), "{rename:?}");

    // A parent with children cannot be deleted even once unfrozen.
    dispatch_action(
        &state,
        Action::FreezeTaxonomyCategory {
            scope: scope(),
            category_id: billing_id,
            frozen: false,
        },
        "req-vocab-7",
        jens(),
    )
    .await
    .unwrap();
    let delete = dispatch_action(
        &state,
        Action::DeleteTaxonomyCategory {
            scope: scope(),
            category_id: billing_id,
        },
        "req-vocab-8",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(delete.status, ActionStatus::Failed), "{delete:?}");

    // The cap: ten roots is the limit for an induced kind.
    for i in 0..9 {
        let r = dispatch_action(
            &state,
            Action::DefineTaxonomyCategory {
                scope: scope(),
                name: format!("cat {i}"),
                description: None,
                vocab_kind: None,
                parent_id: None,
                aliases: None,
            },
            &format!("req-vocab-cap-{i}"),
            jens(),
        )
        .await
        .unwrap();
        assert!(matches!(r.status, ActionStatus::Applied), "{r:?}");
    }
    let over = dispatch_action(
        &state,
        Action::DefineTaxonomyCategory {
            scope: scope(),
            name: "one too many".to_string(),
            description: None,
            vocab_kind: None,
            parent_id: None,
            aliases: None,
        },
        "req-vocab-over",
        jens(),
    )
    .await
    .unwrap();
    assert!(matches!(over.status, ActionStatus::Failed), "{over:?}");

    // Every row names the human.
    let rows = state.store().unwrap().db().list_actions(100).await.unwrap();
    let vocab_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.request_id.starts_with("req-vocab"))
        .collect();
    assert!(vocab_rows.len() >= 10);
    assert!(vocab_rows
        .iter()
        .all(|r| r.user_id.as_deref() == Some("user-jens")));
}

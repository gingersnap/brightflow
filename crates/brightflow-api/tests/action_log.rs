//! End-to-end pins on the action bus: logging, the undo round-trip, and the
//! vocabulary hierarchy rules, all through the public dispatch surface.

#![expect(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "integration tests panic on failure by design and read top to bottom"
)]

use polars::prelude::*;
use std::sync::Arc;

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

/// Execute → capture-undo → undo, through the public surface: a define is
/// logged, applied, undoable, and undone.
#[tokio::test]
async fn define_round_trips_through_undo() {
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;

    let response = dispatch_action(
        &state,
        Action::DefineTaxonomyCategory {
            scope: Scope {
                source_id: SOURCE.to_string(),
                table: TABLE.to_string(),
            },
            name: "billing".to_string(),
            description: Some("charges and invoices".to_string()),
            vocab_kind: None,
            parent_id: None,
            aliases: None,
        },
        "req-define-1",
        Actor::Human {
            user_id: "user-1".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(response.status, ActionStatus::Applied));

    let store = state.store().unwrap();
    let table = store.db().get_table(SOURCE, TABLE).await.unwrap().unwrap();
    assert_eq!(
        store
            .db()
            .get_taxonomy_categories(&table.id)
            .await
            .unwrap()
            .len(),
        1
    );
    let rows = store.db().list_actions(10).await.unwrap();
    let row = rows
        .iter()
        .find(|r| r.request_id == "req-define-1")
        .expect("action-log row must exist");
    assert!(row.undo_json.is_some(), "define must capture an undo op");
    assert_eq!(row.user_id.as_deref(), Some("user-1"));

    // Undo through the public HTTP-handler surface.
    let undone = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(row.id),
    )
    .await
    .unwrap();
    assert!(matches!(undone.0.status, ActionStatus::Undone));
    assert!(store
        .db()
        .get_taxonomy_categories(&table.id)
        .await
        .unwrap()
        .is_empty());
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

    // A subcategory without a parent is a subcategory of `other` (parent 0)...
    let under_other = dispatch_action(
        &state,
        Action::DefineTaxonomyCategory {
            scope: scope(),
            name: "uncovered wish".to_string(),
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
    assert!(
        matches!(under_other.status, ActionStatus::Applied),
        "{under_other:?}"
    );
    assert_eq!(under_other.result["parentId"], 0);

    // ...and one with a parent lands under it.
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

    // Clear the category level: roots, their subcategories and `other`'s
    // subcategories go in one action, frozen or not; undo brings every row
    // back under its original id.
    let store = state.store().unwrap();
    let table = store.db().get_table(SOURCE, TABLE).await.unwrap().unwrap();
    let before = store.db().get_taxonomy_categories(&table.id).await.unwrap();
    assert!(before
        .iter()
        .any(|r| r.kind == "subcategory" && r.parent_id == 0));
    let cleared = dispatch_action(
        &state,
        Action::ClearVocabulary {
            scope: scope(),
            vocab_kind: "category".to_string(),
        },
        "req-vocab-clear",
        jens(),
    )
    .await
    .unwrap();
    assert!(
        matches!(cleared.status, ActionStatus::Applied),
        "{cleared:?}"
    );
    assert_eq!(cleared.result["deleted"], before.len());
    assert!(store
        .db()
        .get_taxonomy_categories(&table.id)
        .await
        .unwrap()
        .is_empty());
    let undone = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(cleared.log_id),
    )
    .await
    .unwrap();
    assert!(matches!(undone.0.status, ActionStatus::Undone));
    let mut after = store.db().get_taxonomy_categories(&table.id).await.unwrap();
    after.sort_by_key(|r| r.id);
    let mut expected = before;
    expected.sort_by_key(|r| r.id);
    let key = |r: &brightflow_store::TaxonomyCategoryRow| {
        (r.id, r.kind.clone(), r.parent_id, r.name.clone(), r.frozen)
    };
    assert_eq!(
        after.iter().map(key).collect::<Vec<_>>(),
        expected.iter().map(key).collect::<Vec<_>>()
    );
}

/// Column semantics through the public bus: every action writes one row at
/// the actor's own layer, a role change clears the KPI flag, a blank label
/// clears, and undo puts the actor's row back — or removes it when the
/// action created it.
#[tokio::test]
async fn column_semantic_actions_write_the_actors_layer_and_round_trip_through_undo() {
    use brightflow_types::{ColumnRole, Layer};

    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let store = state.store().unwrap();
    let scope = || Scope {
        source_id: SOURCE.to_string(),
        table: TABLE.to_string(),
    };
    let human = || Actor::Human {
        user_id: "user-1".to_string(),
    };
    let resolved = |column: &str| {
        let handle = Arc::clone(store);
        let wanted = column.to_string();
        async move {
            handle
                .resolved_columns(SOURCE, TABLE)
                .await
                .unwrap()
                .into_iter()
                .find(|c| c.name == wanted)
                .unwrap()
        }
    };

    // Flagging a column as KPI states only that: the row carries no role.
    dispatch_action(
        &state,
        Action::SetKpi {
            scope: scope(),
            column: "title".to_string(),
            is_kpi: true,
        },
        "req-kpi-title",
        human(),
    )
    .await
    .unwrap();
    let title = resolved("title").await;
    assert_eq!(title.is_kpi, Some(true));
    assert_eq!(title.role, None);
    assert_eq!(
        title
            .resolved_by
            .as_ref()
            .map(|p| (p.layer, p.producer.as_str())),
        Some((Layer::User, "user:user-1"))
    );

    // A label is trimmed.
    dispatch_action(
        &state,
        Action::SetColumnLabel {
            scope: scope(),
            column: "id".to_string(),
            label: Some("  Ticket id ".to_string()),
        },
        "req-label-id",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(resolved("id").await.label.as_deref(), Some("Ticket id"));

    // Moving the KPI column off `measure` clears the flag; the log row
    // carries the previous row for undo.
    let response = dispatch_action(
        &state,
        Action::SetColumnRole {
            scope: scope(),
            column: "title".to_string(),
            role: ColumnRole::Entity,
        },
        "req-role-title",
        human(),
    )
    .await
    .unwrap();
    assert!(matches!(response.status, ActionStatus::Applied));
    assert_eq!(response.result["kpiCleared"], true);
    let title_after_role = resolved("title").await;
    assert_eq!(title_after_role.role, Some(ColumnRole::Entity));
    assert_eq!(title_after_role.is_kpi, Some(false));

    // Undo puts the row back as it was: KPI set, no role.
    let row = store
        .db()
        .list_actions(10)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.request_id == "req-role-title")
        .unwrap();
    let undone = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(row.id),
    )
    .await
    .unwrap();
    assert!(matches!(undone.0.status, ActionStatus::Undone));
    let title_after_undo = resolved("title").await;
    assert_eq!(title_after_undo.role, None);
    assert_eq!(title_after_undo.is_kpi, Some(true));

    // Undoing the action that created a row removes the row.
    let label_row = store
        .db()
        .list_actions(10)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.request_id == "req-label-id")
        .unwrap();
    let undone_label = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(label_row.id),
    )
    .await
    .unwrap();
    assert!(matches!(undone_label.0.status, ActionStatus::Undone));
    assert!(store
        .resolved_columns(SOURCE, TABLE)
        .await
        .unwrap()
        .iter()
        .all(|c| c.name != "id"));

    // A blank description clears rather than storing whitespace.
    dispatch_action(
        &state,
        Action::SetColumnDescription {
            scope: scope(),
            column: "id".to_string(),
            description: Some("   ".to_string()),
        },
        "req-desc-id",
        human(),
    )
    .await
    .unwrap();
    assert_eq!(resolved("id").await.description, None);
}

/// Table settings through the bus: the action writes one row at the
/// actor's layer over whatever a producer said, and undo removes the row
/// the action created.
#[tokio::test]
async fn set_table_settings_writes_the_actors_layer_and_undoes() {
    use brightflow_types::{Layer, TimeGranularity};

    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let store = state.store().unwrap();
    let response = dispatch_action(
        &state,
        Action::SetTableSettings {
            scope: Scope {
                source_id: SOURCE.to_string(),
                table: TABLE.to_string(),
            },
            display_name: Some("  Tickets ".to_string()),
            description: None,
            time_granularity: Some(TimeGranularity::Month),
            comparison_periods: None,
        },
        "req-settings",
        Actor::Human {
            user_id: "user-1".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(response.status, ActionStatus::Applied));
    let resolved = store.resolved_table(SOURCE, TABLE).await.unwrap().unwrap();
    assert_eq!(resolved.display_name.as_deref(), Some("Tickets"));
    assert_eq!(resolved.time_granularity, Some(TimeGranularity::Month));
    assert_eq!(resolved.resolved_by.map(|p| p.layer), Some(Layer::User));

    let row = store
        .db()
        .list_actions(10)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.request_id == "req-settings")
        .unwrap();
    let undone = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(row.id),
    )
    .await
    .unwrap();
    assert!(matches!(undone.0.status, ActionStatus::Undone));
    assert_eq!(store.resolved_table(SOURCE, TABLE).await.unwrap(), None);
}

/// Reset to declared drops a person's rows so the layer beneath shows, and
/// undo brings the rows back.
#[tokio::test]
async fn reset_column_semantics_drops_edits_and_undo_restores_them() {
    use brightflow_types::Layer;

    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let store = state.store().unwrap();
    let scope = || Scope {
        source_id: SOURCE.to_string(),
        table: TABLE.to_string(),
    };
    let human = || Actor::Human {
        user_id: "user-1".to_string(),
    };
    let resolved = |column: &str| {
        let handle = Arc::clone(store);
        let wanted = column.to_string();
        async move {
            handle
                .resolved_columns(SOURCE, TABLE)
                .await
                .unwrap()
                .into_iter()
                .find(|c| c.name == wanted)
        }
    };

    // The detector gives the planted table its base layer (the ingest path
    // used here bypasses the upload handler that would have run it); then a
    // person relabels a column on top.
    brightflow_api::semantics::detect::declare_detected(store, SOURCE, TABLE)
        .await
        .unwrap();
    dispatch_action(
        &state,
        Action::SetColumnLabel {
            scope: scope(),
            column: "title".to_string(),
            label: Some("Headline".to_string()),
        },
        "req-label",
        human(),
    )
    .await
    .unwrap();
    let edited = resolved("title").await.unwrap();
    assert_eq!(edited.label.as_deref(), Some("Headline"));
    assert_eq!(
        edited.resolved_by.as_ref().map(|p| p.layer),
        Some(Layer::User)
    );

    let response = dispatch_action(
        &state,
        Action::ResetColumnSemantics {
            scope: scope(),
            column: "title".to_string(),
        },
        "req-reset",
        human(),
    )
    .await
    .unwrap();
    assert!(matches!(response.status, ActionStatus::Applied));
    assert_eq!(response.result["removed"], 1);
    let reset = resolved("title").await.unwrap();
    assert_eq!(reset.label, None);
    assert_eq!(
        reset.resolved_by.as_ref().map(|p| p.layer),
        Some(Layer::Detected)
    );

    let row = store
        .db()
        .list_actions(10)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.request_id == "req-reset")
        .unwrap();
    let undone = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(row.id),
    )
    .await
    .unwrap();
    assert!(matches!(undone.0.status, ActionStatus::Undone));
    let restored = resolved("title").await.unwrap();
    assert_eq!(restored.label.as_deref(), Some("Headline"));
    assert_eq!(
        restored.resolved_by.as_ref().map(|p| p.layer),
        Some(Layer::User)
    );
}

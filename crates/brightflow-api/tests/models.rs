//! A model end to end: the recipe runs over the input, the output is an
//! ordinary table, the input's semantics carry over under the model's own
//! producer, a rebuild replaces rather than appends, a model over a model
//! follows its feeder, and an empty result still leaves a table.

#![expect(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "integration tests panic on failure by design and read top to bottom"
)]

use std::sync::Arc;

use polars::prelude::*;

use brightflow_api::models::{rebuild, rebuild_dependents};
use brightflow_api::state::AppState;
use brightflow_store::{ModelRow, ModelVersionRow, ParquetStore};
use brightflow_test_support::{copy_template, TestWorkspace};
use brightflow_types::{
    AggSpec, Aggregation, ColumnRole, DerivedColumn, DerivedExpr, FilterOp, Layer, ModelRecipe,
    Operation, TimeGranularity,
};

const SOURCE: &str = "test-source";
const INPUT: &str = "orders";

async fn state_with_orders(ws: &TestWorkspace) -> AppState {
    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();
    let parquet_path = ws.root().join("orders.parquet");
    let mut df = df!(
        "order_date" => &["2026-05-01", "2026-05-20", "2026-06-03", "2026-06-15"],
        "region" => &["EU", "US", "EU", "EU"],
        "revenue" => &[10.0_f64, 20.0, 30.0, 40.0],
    )
    .unwrap();
    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();
    store
        .ingest_parquet(SOURCE, INPUT, &parquet_path, None)
        .await
        .unwrap();
    AppState::with_store(store).await
}

/// Plant a model directly in the catalog (the bus is exercised in §4's
/// test); the output row exists empty until the first build.
async fn plant_model(state: &AppState, input: &str, output: &str, recipe: &ModelRecipe) -> String {
    let db = state.store().unwrap().db();
    let input_row = db.get_table(SOURCE, input).await.unwrap().unwrap();
    let output_row = db.create_table(output, SOURCE).await.unwrap();
    let id = uuid::Uuid::now_v7().to_string();
    db.insert_model(
        &ModelRow {
            id: id.clone(),
            output_table_id: output_row.id,
            input_table_id: Some(input_row.id),
            current_version: 1,
            created_by: Some("user:test".to_string()),
            created_at: 1,
            updated_at: 1,
        },
        &ModelVersionRow {
            model_id: id.clone(),
            version: 1,
            recipe_json: serde_json::to_string(recipe).unwrap(),
            client_spec: None,
            created_by: Some("user:test".to_string()),
            created_at: 1,
        },
    )
    .await
    .unwrap();
    id
}

fn by_month() -> ModelRecipe {
    ModelRecipe::new(vec![
        Operation::WithColumns {
            columns: vec![DerivedColumn {
                name: "order_date__month".to_string(),
                expr: DerivedExpr::Period {
                    column: "order_date".to_string(),
                    granularity: TimeGranularity::Month,
                },
            }],
        },
        Operation::GroupBy {
            by: vec!["order_date__month".to_string(), "region".to_string()],
            aggs: vec![AggSpec {
                column: "revenue".to_string(),
                function: Aggregation::Sum,
                alias: Some("sum".to_string()),
            }],
        },
    ])
}

#[tokio::test]
async fn a_build_materialises_the_chain_and_carries_the_inputs_semantics() {
    let ws = copy_template().unwrap();
    let state = state_with_orders(&ws).await;
    let store = Arc::clone(state.store().unwrap());

    // A person has named the region column; the model must keep that name.
    brightflow_api::actions::dispatch_action(
        &state,
        brightflow_api::actions::types::Action::SetColumnLabel {
            scope: brightflow_api::actions::types::Scope {
                source_id: SOURCE.to_string(),
                table: INPUT.to_string(),
            },
            column: "region".to_string(),
            label: Some("Sales region".to_string()),
        },
        "req-label",
        brightflow_api::actions::Actor::Human {
            user_id: "u1".to_string(),
        },
    )
    .await
    .unwrap();

    let model_id = plant_model(&state, INPUT, "revenue_by_month", &by_month()).await;
    let outcome = rebuild(&state, &model_id, "create").await.unwrap();
    assert_eq!(outcome.rows, 3, "two EU months plus one US month");
    assert_eq!(outcome.table, "revenue_by_month");

    let df = store.read_table(SOURCE, "revenue_by_month").await.unwrap();
    assert_eq!(df.height(), 3);
    assert!(df.column("order_date__month").is_ok());
    assert!(df.column("sum").is_ok());

    let columns = store
        .resolved_columns(SOURCE, "revenue_by_month")
        .await
        .unwrap();
    let region = columns.iter().find(|c| c.name == "region").unwrap();
    assert_eq!(region.label.as_deref(), Some("Sales region"));
    let by = region.resolved_by.as_ref().unwrap();
    assert_eq!(by.layer, Layer::Declared);
    assert_eq!(by.producer, format!("model:{model_id}"));
    let month = columns
        .iter()
        .find(|c| c.name == "order_date__month")
        .unwrap();
    assert_eq!(month.role, Some(ColumnRole::Time));
    let sum = columns.iter().find(|c| c.name == "sum").unwrap();
    assert_eq!(sum.role, Some(ColumnRole::Measure));

    // The build is on record and finished.
    let build = store
        .db()
        .get_model_build(&outcome.build_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(build.status, "completed");
    assert_eq!(build.rows, Some(3));

    // A second build replaces: same row count, one file.
    rebuild(&state, &model_id, "manual").await.unwrap();
    let rebuilt = store.read_table(SOURCE, "revenue_by_month").await.unwrap();
    assert_eq!(rebuilt.height(), 3);
    let table = store
        .db()
        .get_table(SOURCE, "revenue_by_month")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        store.db().list_table_files(&table.id).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn a_model_over_a_model_rebuilds_after_its_feeder_and_empty_results_keep_the_table() {
    let ws = copy_template().unwrap();
    let state = state_with_orders(&ws).await;
    let store = Arc::clone(state.store().unwrap());

    let first = plant_model(&state, INPUT, "revenue_by_month", &by_month()).await;
    rebuild(&state, &first, "create").await.unwrap();
    let second = plant_model(
        &state,
        "revenue_by_month",
        "eu_months",
        &ModelRecipe::new(vec![Operation::Filter {
            column: "region".to_string(),
            op: FilterOp::Eq,
            value: serde_json::json!("EU"),
        }]),
    )
    .await;
    rebuild(&state, &second, "create").await.unwrap();
    assert_eq!(
        store
            .read_table(SOURCE, "eu_months")
            .await
            .unwrap()
            .height(),
        2
    );

    // Writing the root table again rebuilds both, feeder first.
    rebuild_dependents(&state, SOURCE, INPUT).await;
    let first_build = store
        .db()
        .latest_model_build(&first)
        .await
        .unwrap()
        .unwrap();
    let second_build = store
        .db()
        .latest_model_build(&second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_build.triggered_by, "sync");
    assert_eq!(second_build.triggered_by, "sync");
    assert!(second_build.started_at >= first_build.started_at);

    // A filter that matches nothing leaves a zero-row table, not no table.
    let none = plant_model(
        &state,
        INPUT,
        "no_rows",
        &ModelRecipe::new(vec![Operation::Filter {
            column: "region".to_string(),
            op: FilterOp::Eq,
            value: serde_json::json!("Mars"),
        }]),
    )
    .await;
    let outcome = rebuild(&state, &none, "create").await.unwrap();
    assert_eq!(outcome.rows, 0);
    assert!(store
        .db()
        .get_table(SOURCE, "no_rows")
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        store.read_table(SOURCE, "no_rows").await.unwrap().height(),
        0
    );
}

/// The bus round trip, both actors: create → applied and the table exists →
/// update → rows change → undo → rows back → delete → gone → undo → back
/// under the same model id; an agent under auto_apply gets `applied` for
/// create and `proposed` for rebuild.
#[tokio::test]
async fn model_actions_round_trip_through_the_bus_for_both_actors() {
    use brightflow_api::actions::types::{Action, ActionStatus, Scope};
    use brightflow_api::actions::{dispatch_action, Actor};

    let ws = copy_template().unwrap();
    let state = state_with_orders(&ws).await;
    let store = Arc::clone(state.store().unwrap());
    let input_scope = || Scope {
        source_id: SOURCE.to_string(),
        table: INPUT.to_string(),
    };
    let human = || Actor::Human {
        user_id: "u1".to_string(),
    };

    let response = dispatch_action(
        &state,
        Action::CreateModel {
            scope: input_scope(),
            name: "eu_orders".to_string(),
            recipe: ModelRecipe::new(vec![Operation::Filter {
                column: "region".to_string(),
                op: FilterOp::Eq,
                value: serde_json::json!("EU"),
            }]),
            client_spec: Some(serde_json::json!({ "version": 1 })),
        },
        "req-create",
        human(),
    )
    .await
    .unwrap();
    assert!(matches!(response.status, ActionStatus::Applied));
    let model_id = response.result["model_id"].as_str().unwrap().to_string();
    assert_eq!(response.result["rows"], 3);
    assert_eq!(
        store
            .read_table(SOURCE, "eu_orders")
            .await
            .unwrap()
            .height(),
        3
    );

    // A bad recipe never leaves a half-model.
    let bad = dispatch_action(
        &state,
        Action::CreateModel {
            scope: input_scope(),
            name: "broken".to_string(),
            recipe: ModelRecipe::new(vec![Operation::Sort {
                by: "nope".to_string(),
                descending: false,
            }]),
            client_spec: None,
        },
        "req-create-bad",
        human(),
    )
    .await
    .unwrap();
    assert!(matches!(bad.status, ActionStatus::Failed));
    assert!(store
        .db()
        .get_table(SOURCE, "broken")
        .await
        .unwrap()
        .is_none());

    let output_scope = || Scope {
        source_id: SOURCE.to_string(),
        table: "eu_orders".to_string(),
    };
    let updated = dispatch_action(
        &state,
        Action::UpdateModel {
            scope: output_scope(),
            model_id: model_id.clone(),
            recipe: ModelRecipe::new(vec![Operation::Filter {
                column: "region".to_string(),
                op: FilterOp::Eq,
                value: serde_json::json!("US"),
            }]),
            client_spec: None,
        },
        "req-update",
        human(),
    )
    .await
    .unwrap();
    assert!(matches!(updated.status, ActionStatus::Applied));
    assert_eq!(updated.result["version"], 2);
    assert_eq!(
        store
            .read_table(SOURCE, "eu_orders")
            .await
            .unwrap()
            .height(),
        1
    );

    let rows = store.db().list_actions(10).await.unwrap();
    let update_row = rows.iter().find(|r| r.request_id == "req-update").unwrap();
    let undone = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(update_row.id),
    )
    .await
    .unwrap();
    assert!(matches!(undone.0.status, ActionStatus::Undone));
    assert_eq!(
        store
            .read_table(SOURCE, "eu_orders")
            .await
            .unwrap()
            .height(),
        3
    );
    let model = store.db().get_model(&model_id).await.unwrap().unwrap();
    assert_eq!(model.current_version, 1);

    // An agent under auto_apply: create applies, rebuild is a proposal.
    let agent = || Actor::Agent {
        run_id: 7,
        auto_apply: true,
    };
    let by_agent = dispatch_action(
        &state,
        Action::CreateModel {
            scope: input_scope(),
            name: "agent_summary".to_string(),
            recipe: by_month(),
            client_spec: None,
        },
        "req-agent-create",
        agent(),
    )
    .await
    .unwrap();
    assert!(matches!(by_agent.status, ActionStatus::Applied));
    let agent_table = store
        .db()
        .get_table(SOURCE, "agent_summary")
        .await
        .unwrap()
        .unwrap();
    let agent_model = store
        .db()
        .get_model_by_output(&agent_table.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(agent_model.created_by.as_deref(), Some("agent:7"));
    let proposed = dispatch_action(
        &state,
        Action::RebuildModel {
            scope: Scope {
                source_id: SOURCE.to_string(),
                table: "agent_summary".to_string(),
            },
            model_id: agent_model.id.clone(),
        },
        "req-agent-rebuild",
        agent(),
    )
    .await
    .unwrap();
    assert!(matches!(proposed.status, ActionStatus::Proposed));

    // Delete and undo bring the model back under its own id.
    let deleted = dispatch_action(
        &state,
        Action::DeleteModel {
            scope: output_scope(),
            model_id: model_id.clone(),
        },
        "req-delete",
        human(),
    )
    .await
    .unwrap();
    assert!(matches!(deleted.status, ActionStatus::Applied));
    assert!(store
        .db()
        .get_table(SOURCE, "eu_orders")
        .await
        .unwrap()
        .is_none());
    assert!(store.db().get_model(&model_id).await.unwrap().is_none());

    let later_rows = store.db().list_actions(20).await.unwrap();
    let delete_row = later_rows
        .iter()
        .find(|r| r.request_id == "req-delete")
        .unwrap();
    let restored_response = brightflow_api::actions::handlers::undo(
        axum::extract::State(state.clone()),
        axum::extract::Path(delete_row.id),
    )
    .await
    .unwrap();
    assert!(matches!(restored_response.0.status, ActionStatus::Undone));
    assert_eq!(
        store
            .read_table(SOURCE, "eu_orders")
            .await
            .unwrap()
            .height(),
        3
    );
    let restored = store.db().get_model(&model_id).await.unwrap().unwrap();
    assert_eq!(restored.current_version, 1);
    assert_eq!(
        store
            .db()
            .list_model_versions(&model_id)
            .await
            .unwrap()
            .len(),
        2
    );

    // The listing names both tables and the last build.
    let listed = brightflow_api::models::handlers::list_models(
        axum::extract::State(state.clone()),
        axum::extract::Path(SOURCE.to_string()),
    )
    .await
    .unwrap();
    let eu = listed.0.iter().find(|m| m.table == "eu_orders").unwrap();
    assert_eq!(eu.input_table.as_deref(), Some(INPUT));
    assert_eq!(eu.last_build.as_ref().unwrap().status, "completed");
}

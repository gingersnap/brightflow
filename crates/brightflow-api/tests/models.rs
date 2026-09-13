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

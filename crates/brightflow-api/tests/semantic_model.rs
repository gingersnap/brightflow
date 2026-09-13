//! A pasted semantic model becomes declared rows: the import route applies a
//! document per table under `document:{name}`, reports datasets with no
//! table, refuses a malformed model, and a dry run writes nothing. The
//! export route then rebuilds a document from the resolved rows, and the
//! table-semantics route shows the resolved table with the layer behind it.

#![expect(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "integration tests panic on failure by design and read top to bottom"
)]

use axum::extract::{Path, Query, State};
use axum::Json;
use polars::prelude::*;

use brightflow_api::semantics::handlers::{
    export_semantic_model, get_table_semantics, import_semantic_model,
};
use brightflow_api::semantics::types::{ImportQuery, LayersQuery};
use brightflow_api::state::AppState;
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};
use brightflow_types::{Layer, LogicalType};

const SOURCE: &str = "test-source";

/// A store on a copy of the committed workspace with one planted table.
async fn state_with_planted_table(ws: &TestWorkspace) -> AppState {
    let store = ParquetStore::new(ws.paths.store(), &ws.paths.litehouse_url())
        .await
        .unwrap();
    let parquet_path = ws.root().join("issues.parquet");
    let mut df = df!(
        "id" => &[1_i64, 2, 3],
        "title" => &["a", "b", "c"],
        "reactions_total" => &[4_i64, 5, 6],
    )
    .unwrap();
    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();
    store
        .ingest_parquet(SOURCE, "issues", &parquet_path, None)
        .await
        .unwrap();
    AppState::with_store(store).await
}

fn model() -> serde_json::Value {
    serde_json::json!({
        "name": "hand-written",
        "datasets": [
            {"name": "issues", "source": "issues", "description": "Issues of the repository",
             "fields": {
                "id": {"datatype": "Integer", "brightflow": {"role": "ignored"}},
                "title": {"datatype": "String", "brightflow": {"role": "ignored"}},
                "reactions_total": {"datatype": "Integer", "description": "Reactions",
                                    "brightflow": {"role": "measure", "is_kpi": true}},
                "ghost": {"datatype": "String"}
             },
             "brightflow": {"display_name": "Issues", "time_granularity": "week"}},
            {"name": "not_here", "source": "not_here", "fields": {"id": {"datatype": "Integer"}}}
        ],
        "metrics": {"reactions": {"expression": {"dataset": "issues", "column": "reactions_total", "aggregation": "sum"}}}
    })
}

#[tokio::test]
async fn import_applies_the_model_per_table_and_export_rebuilds_it() {
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let store = state.store().unwrap();

    // A dry run reports without writing.
    let dry = import_semantic_model(
        State(state.clone()),
        Path(SOURCE.to_string()),
        Query(ImportQuery {
            dry_run: Some(true),
        }),
        Json(model()),
    )
    .await
    .unwrap()
    .0;
    assert!(dry.dry_run);
    assert_eq!(dry.missing_tables, vec!["not_here".to_string()]);
    assert_eq!(dry.applied.len(), 1);
    assert_eq!(
        dry.applied[0].columns_without_data,
        vec!["ghost".to_string()]
    );
    assert!(store
        .column_opinions(SOURCE, "issues")
        .await
        .unwrap()
        .is_empty());

    let response = import_semantic_model(
        State(state.clone()),
        Path(SOURCE.to_string()),
        Query(ImportQuery::default()),
        Json(model()),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(response.producer, "document:hand-written");
    assert_eq!(response.applied[0].columns, 4);
    assert_eq!(response.applied[0].metrics, 1);
    assert_eq!(
        response.applied[0].columns_without_data,
        vec!["ghost".to_string()]
    );

    let resolved = store.resolved_columns(SOURCE, "issues").await.unwrap();
    let reactions = resolved
        .iter()
        .find(|c| c.name == "reactions_total")
        .unwrap();
    assert_eq!(reactions.is_kpi, Some(true));
    assert_eq!(reactions.datatype, Some(LogicalType::Integer));
    assert_eq!(
        reactions
            .resolved_by
            .as_ref()
            .map(|p| (p.layer, p.producer.as_str())),
        Some((Layer::Declared, "document:hand-written"))
    );
    let table = store
        .resolved_table(SOURCE, "issues")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(table.display_name.as_deref(), Some("Issues"));

    // Export rebuilds a document from the resolved rows.
    let exported = export_semantic_model(State(state.clone()), Path(SOURCE.to_string()))
        .await
        .unwrap()
        .0;
    let issues = exported.document.semantic_model[0]
        .datasets
        .iter()
        .find(|d| d.name == "issues")
        .unwrap();
    assert_eq!(
        issues.description.as_deref(),
        Some("Issues of the repository")
    );
    assert_eq!(exported.document.semantic_model[0].metrics.len(), 1);

    // The table route shows the resolved table and, with `?layers=1`, the
    // declared row the document wrote.
    let semantics = get_table_semantics(
        State(state.clone()),
        Path((SOURCE.to_string(), "issues".to_string())),
        Query(LayersQuery { layers: Some(true) }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(semantics.table_name, "issues");
    assert_eq!(
        semantics
            .table
            .as_ref()
            .and_then(|t| t.display_name.as_deref()),
        Some("Issues")
    );
    let layers = semantics.layers.unwrap();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].provenance.layer, Layer::Declared);
    assert_eq!(layers[0].provenance.producer, "document:hand-written");
    assert_eq!(layers[0].display_name.as_deref(), Some("Issues"));
}

#[tokio::test]
async fn a_malformed_model_is_refused() {
    let ws = copy_template().unwrap();
    let state = state_with_planted_table(&ws).await;
    let bad = serde_json::json!({
        "name": "bad",
        "datasets": [{"name": "issues", "source": "issues",
                      "fields": {"id": {"datatype": "Timestamp"}}}]
    });
    let err = import_semantic_model(
        State(state.clone()),
        Path(SOURCE.to_string()),
        Query(ImportQuery::default()),
        Json(bad),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Timestamp"), "{err}");
    let two_models = serde_json::json!({"version": "0.2.0", "semantic_model": [model(), model()]});
    let two_err = import_semantic_model(
        State(state),
        Path(SOURCE.to_string()),
        Query(ImportQuery::default()),
        Json(two_models),
    )
    .await
    .unwrap_err();
    assert!(
        two_err.to_string().contains("2 semantic models"),
        "{two_err}"
    );
}

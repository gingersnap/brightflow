//! Two functions materialising into one table at the same time must both
//! land. Before the per-table lock, both rebuilt the frame from the same
//! `tables.version` and the loser of the optimistic check got exactly one
//! retry — a third writer (a sync) in the same window failed the run.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests panic on failure by design"
)]

use std::sync::Arc;

use polars::prelude::*;

use brightflow_api::enrichment::runner::{materialize, prepare_run_inputs, RunSpec};
use brightflow_api::enrichment::vocab;
use brightflow_api::state::AppState;
use brightflow_engine::enrichment::{FunctionSpec, TicketClassifySpec, TicketExtractSpec};
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};

const SOURCE: &str = "test-source";
const TABLE: &str = "issues";

fn classify_spec() -> FunctionSpec {
    FunctionSpec::TicketClassify(TicketClassifySpec {
        text_columns: vec!["title".to_string()],
        language_column: None,
        provider_id: "p".to_string(),
        model: None,
        categories: vec![],
        subcategories: vec![],
    })
}

fn extract_spec() -> FunctionSpec {
    FunctionSpec::TicketExtract(TicketExtractSpec {
        text_columns: vec!["title".to_string()],
        language_column: None,
        provider_id: "p".to_string(),
        model: None,
        products: vec![],
        competitors: vec![],
        feedback_categories: vec![],
    })
}

async fn state_with_table(ws: &TestWorkspace) -> AppState {
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

/// Create a promoted function and fill its cache for every row so
/// `materialize` has something to write. Returns the id and runnable spec.
async fn function_with_cache(
    state: &AppState,
    name: &str,
    spec: FunctionSpec,
    cell_value: serde_json::Value,
) -> (String, RunSpec) {
    let store = state.store().unwrap();
    let table = store.db().get_table(SOURCE, TABLE).await.unwrap().unwrap();
    let row = store
        .db()
        .create_enrichment_function(
            &table.id,
            name,
            spec.kind_str(),
            "promoted",
            &serde_json::to_string(&spec).unwrap(),
        )
        .await
        .unwrap();
    let run = vocab::run_spec_for(store, &table.id, spec).await.unwrap();
    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let shash = run.spec_hash();
    for input in prepare_run_inputs(&df, &run).unwrap() {
        store
            .db()
            .upsert_cached_cell(
                &row.id,
                &shash,
                &input.hash,
                "ok",
                Some(&cell_value.to_string()),
                None,
                Some(1),
                Some(1),
                None,
                1,
            )
            .await
            .unwrap();
    }
    (row.id, run)
}

#[tokio::test]
async fn concurrent_materializations_on_one_table_both_land() {
    let ws = copy_template().unwrap();
    let state = state_with_table(&ws).await;
    let store = Arc::clone(state.store().unwrap());

    let classify_cell = serde_json::json!({
        "summary": "a summary",
        "category_id": 0,
        "subcategory_id": 0,
        "sentiment": "neutral",
    });
    let (id_a, run_a) =
        function_with_cache(&state, "classify", classify_spec(), classify_cell).await;
    let (id_b, run_b) = function_with_cache(
        &state,
        "extract",
        extract_spec(),
        serde_json::json!({ "mentions": [] }),
    )
    .await;

    let (ra, rb) = tokio::join!(
        materialize(&state, &store, SOURCE, TABLE, &id_a, "classify", &run_a),
        materialize(&state, &store, SOURCE, TABLE, &id_b, "extract", &run_b),
    );
    ra.unwrap();
    rb.unwrap();

    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let summary = df.column("summary").unwrap().str().unwrap();
    assert_eq!(summary.get(0), Some("a summary"));
    let count = df.column("mention_count").unwrap().i32().unwrap();
    assert_eq!(count.get(2), Some(0));
    assert!(df.column("classify__status").is_ok());
    assert!(df.column("extract__status").is_ok());
}

/// Materialisation declares what its columns mean as the function's own
/// `declared`-layer rows. A person's edit above that layer survives a
/// re-run, and the function's description still shows through beneath it.
#[tokio::test]
async fn materialize_declares_output_semantics_without_overwriting_edits() {
    use brightflow_types::{ColumnRole, Layer};

    let ws = copy_template().unwrap();
    let state = state_with_table(&ws).await;
    let store = Arc::clone(state.store().unwrap());
    let table = store.db().get_table(SOURCE, TABLE).await.unwrap().unwrap();

    let classify_cell = serde_json::json!({
        "summary": "a summary",
        "category_id": 0,
        "subcategory_id": 0,
        "sentiment": "neutral",
    });
    let (id, run) = function_with_cache(&state, "classify", classify_spec(), classify_cell).await;
    materialize(&state, &store, SOURCE, TABLE, &id, "classify", &run)
        .await
        .unwrap();

    let resolved = store.resolved_columns(SOURCE, TABLE).await.unwrap();
    let find = |name: &str| resolved.iter().find(|r| r.name == name).unwrap();
    assert_eq!(find("summary").role, Some(ColumnRole::Ignored));
    assert_eq!(find("summary").label.as_deref(), Some("Summary"));
    assert_eq!(find("category").role, Some(ColumnRole::Dimension));
    assert!(find("sentiment").description.is_some());
    assert_eq!(find("classify__status").role, Some(ColumnRole::Ignored));
    assert_eq!(
        find("category")
            .resolved_by
            .as_ref()
            .map(|p| (p.layer, p.producer.as_str())),
        Some((Layer::Declared, "enrichment:classify"))
    );

    // The in-memory overrides Explore reads follow the rows.
    let live = state
        .schema_overrides
        .get(&format!("{SOURCE}|{TABLE}"))
        .map(|v| v.value().clone())
        .unwrap_or_default();
    assert!(live
        .iter()
        .any(|o| o.column_name == "category" && o.label.as_deref() == Some("Category")));

    // A person renames the column at the user layer; a second
    // materialisation keeps the name and the function's description.
    rename_category(&store, &table.id).await;
    materialize(&state, &store, SOURCE, TABLE, &id, "classify", &run)
        .await
        .unwrap();
    let after_rerun = store.resolved_columns(SOURCE, TABLE).await.unwrap();
    let category = after_rerun.iter().find(|r| r.name == "category").unwrap();
    assert_eq!(category.label.as_deref(), Some("Problem area"));
    assert!(category.description.is_some());
    assert_eq!(
        category.resolved_by.as_ref().map(|p| p.layer),
        Some(Layer::User)
    );
}

/// A person's rename of `category`, written at the user layer.
async fn rename_category(store: &ParquetStore, table_id: &str) {
    use brightflow_types::{ColumnOpinion, Layer, Provenance};
    let mut edit = ColumnOpinion::empty(
        "category",
        Provenance {
            layer: Layer::User,
            producer: "user:1".to_string(),
            version: None,
            hash: None,
        },
    );
    edit.ext.label = Some("Problem area".to_string());
    store
        .db()
        .write_column_opinion(table_id, &edit)
        .await
        .unwrap();
}

//! Two functions materialising into one table at the same time must both
//! land. Before the per-table lock, both rebuilt the frame from the same
//! `tables.version` and the loser of the optimistic check got exactly one
//! retry — a third writer (a sync) in the same window failed the run.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "integration tests panic on failure by design"
)]

use std::sync::Arc;

use polars::prelude::*;

use brightflow_api::enrichment::runner::{llm_spec_hash, materialize, prepare_inputs, RunSpec};
use brightflow_api::state::AppState;
use brightflow_engine::enrichment::{LlmPromptSpec, OutputField, OutputType};
use brightflow_store::ParquetStore;
use brightflow_test_support::{copy_template, TestWorkspace};

const SOURCE: &str = "test-source";
const TABLE: &str = "issues";

fn spec(output: &str) -> LlmPromptSpec {
    LlmPromptSpec {
        input_columns: vec!["title".to_string()],
        prompt_template: format!("{{{{col:title}}}} -> {output}"),
        outputs: vec![OutputField {
            name: output.to_string(),
            dtype: OutputType::String,
            description: String::new(),
        }],
        provider_id: "p".to_string(),
        model: None,
    }
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
/// `materialize` has something to write.
async fn function_with_cache(state: &AppState, spec: &LlmPromptSpec) -> String {
    let store = state.store().unwrap();
    let table = store.db().get_table(SOURCE, TABLE).await.unwrap().unwrap();
    let name = spec.outputs[0].name.clone();
    let row = store
        .db()
        .create_enrichment_function(
            &table.id,
            &name,
            "llm_prompt",
            "promoted",
            &serde_json::to_string(&brightflow_engine::enrichment::FunctionSpec::LlmPrompt(
                spec.clone(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let shash = llm_spec_hash(spec);
    for input in prepare_inputs(&df, spec).unwrap() {
        let value =
            serde_json::json!({ name.clone(): format!("{name}:{}", input.rendered["title"]) });
        store
            .db()
            .upsert_cached_cell(
                &row.id,
                &shash,
                &input.hash,
                "ok",
                Some(&value.to_string()),
                None,
                Some(1),
                Some(1),
                None,
                1,
            )
            .await
            .unwrap();
    }
    row.id
}

#[tokio::test]
async fn concurrent_materializations_on_one_table_both_land() {
    let ws = copy_template().unwrap();
    let state = state_with_table(&ws).await;
    let store = Arc::clone(state.store().unwrap());

    let spec_a = spec("alpha");
    let spec_b = spec("beta");
    let id_a = function_with_cache(&state, &spec_a).await;
    let id_b = function_with_cache(&state, &spec_b).await;

    let run_a = RunSpec::LlmPrompt(spec_a.clone());
    let run_b = RunSpec::LlmPrompt(spec_b.clone());
    let (ra, rb) = tokio::join!(
        materialize(&state, &store, SOURCE, TABLE, &id_a, "alpha", &run_a),
        materialize(&state, &store, SOURCE, TABLE, &id_b, "beta", &run_b),
    );
    ra.unwrap();
    rb.unwrap();

    let df = store.read_table(SOURCE, TABLE).await.unwrap();
    let alpha = df.column("alpha").unwrap().str().unwrap();
    let beta = df.column("beta").unwrap().str().unwrap();
    assert_eq!(alpha.get(0), Some("alpha:a"));
    assert_eq!(beta.get(2), Some("beta:c"));
    assert!(df.column("alpha__status").is_ok());
    assert!(df.column("beta__status").is_ok());
}

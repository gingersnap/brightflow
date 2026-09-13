//! Models: derived tables built by running a recipe over an input table.
//!
//! A build reads the model's current recipe, runs the chain over the
//! input's Parquet, writes the result as an ordinary table under the same
//! source, gives that table a detected base layer, and declares what the
//! model itself knows about the columns (`semantics`). It records itself as
//! a build row and pushes job events, so Activity shows it like a sync.
//! Builds of one output serialise on the same lock enrichment uses for that
//! table. A failed build leaves the previous output as it was.
//!
//! Who may create, change or delete a model is the action bus's decision
//! (`actions::exec::models`); this module only builds.

pub mod deps;
pub mod semantics;

use std::sync::Arc;

use brightflow_store::{ModelBuildRow, ModelRow, ParquetStore, TableRow};
use brightflow_types::ModelRecipe;

use crate::analytics::executor::run_chain;
use crate::analytics::session::DatasetData;
use crate::shared::{AppError, AppResult};
use crate::state::{cache_key, AppState};

/// What a build produced.
#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub build_id: String,
    pub rows: i64,
    pub source_id: String,
    pub table: String,
}

/// The model, its output table row and its input table row, resolved
/// together so every caller fails the same way when one is missing.
pub struct ModelContext {
    pub model: ModelRow,
    pub output: TableRow,
    pub input: TableRow,
}

pub async fn context(store: &ParquetStore, model_id: &str) -> AppResult<ModelContext> {
    let db = store.db();
    let model = db
        .get_model(model_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("model '{model_id}' not found")))?;
    let output = db
        .get_table_by_id(&model.output_table_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("model '{model_id}' has no output table")))?;
    let input_id = model.input_table_id.clone().ok_or_else(|| {
        AppError::BadRequest(format!(
            "model '{}' cannot build: its input table has been deleted",
            output.name
        ))
    })?;
    let input = db.get_table_by_id(&input_id).await?.ok_or_else(|| {
        AppError::NotFound(format!("input table of model '{}' not found", output.name))
    })?;
    Ok(ModelContext {
        model,
        output,
        input,
    })
}

/// The model's current recipe.
pub async fn current_recipe(store: &ParquetStore, model: &ModelRow) -> AppResult<ModelRecipe> {
    let version = store
        .db()
        .get_model_version(&model.id, model.current_version)
        .await?
        .ok_or_else(|| {
            AppError::Internal(format!(
                "model '{}' has no version {}",
                model.id, model.current_version
            ))
        })?;
    serde_json::from_str(&version.recipe_json)
        .map_err(|e| AppError::Internal(format!("stored recipe is not readable: {e}")))
}

/// Build (or rebuild) a model's output from its current recipe. `trigger`
/// names why, for the build row: `create`, `update`, `sync`, `manual`,
/// `undo`.
pub async fn rebuild(state: &AppState, model_id: &str, trigger: &str) -> AppResult<BuildOutcome> {
    let store = Arc::clone(state.require_store()?);
    let ctx = context(&store, model_id).await?;
    let recipe = current_recipe(&store, &ctx.model).await?;
    let source_id = ctx.output.source_id.clone();
    let output = ctx.output.name.clone();

    let lock = state.materialize_lock(&cache_key(&source_id, &output));
    let _guard = lock.lock().await;

    let build = ModelBuildRow {
        id: uuid::Uuid::now_v7().to_string(),
        model_id: model_id.to_string(),
        version: ctx.model.current_version,
        status: "running".to_string(),
        triggered_by: trigger.to_string(),
        rows: None,
        error: None,
        started_at: now(),
        finished_at: None,
    };
    store.db().insert_model_build(&build).await?;
    crate::jobs::emit_model_job(state, &build.id).await;

    let result = build_once(state, &store, &ctx, &recipe).await;
    let (status, rows, error) = match &result {
        Ok(rows) => ("completed", Some(*rows), None),
        Err(e) => ("failed", None, Some(e.to_string())),
    };
    store
        .db()
        .finish_model_build(&build.id, status, rows, error.as_deref(), now())
        .await?;
    crate::jobs::emit_model_job(state, &build.id).await;

    let built_rows = result?;
    Ok(BuildOutcome {
        build_id: build.id,
        rows: built_rows,
        source_id,
        table: output,
    })
}

/// The build proper: chain → frame → table → semantics.
async fn build_once(
    state: &AppState,
    store: &ParquetStore,
    ctx: &ModelContext,
    recipe: &ModelRecipe,
) -> AppResult<i64> {
    let source_id = &ctx.output.source_id;
    let files = store
        .get_table_parquet_paths(&ctx.input.source_id, &ctx.input.name)
        .await?;
    let data = DatasetData::Parquet { files };
    let operations = recipe.operations.clone();
    let df = tokio::task::spawn_blocking(move || run_chain(&data, &operations))
        .await
        .map_err(|e| AppError::Internal(format!("build task failed: {e}")))??;

    let row = store.write_table(source_id, &ctx.output.name, df).await?;
    state.refresh_table_index().await;

    // A detected base layer first, then the model's own opinion above it.
    if let Err(e) = crate::semantics::detect::declare_detected_if_undescribed(
        store,
        source_id,
        &ctx.output.name,
    )
    .await
    {
        tracing::warn!(
            "detection for model output '{}' failed: {e}",
            ctx.output.name
        );
    }
    let input_columns = store
        .resolved_columns(&ctx.input.source_id, &ctx.input.name)
        .await?;
    let input_schema = ctx.input.schema().unwrap_or_default();
    let output_schema = row.schema().unwrap_or_default();
    let fields = semantics::carry_over(
        &input_schema,
        &input_columns,
        &output_schema,
        &recipe.operations,
    );
    semantics::declare_model_output(
        store,
        &ctx.model.id,
        source_id,
        &ctx.output.name,
        &ctx.input.name,
        fields,
    )
    .await?;
    Ok(row.total_rows)
}

/// Rebuild every model fed by `table`, directly or through other models,
/// in feeding order. Errors are logged per model and never stop the rest;
/// a broken model is visible as a failed build in Activity.
pub async fn rebuild_dependents(state: &AppState, source_id: &str, table: &str) {
    let Some(store) = state.store() else { return };
    let Ok(Some(row)) = store.db().get_table(source_id, table).await else {
        return;
    };
    let Ok(listed) = store.db().list_models_for_source(source_id).await else {
        return;
    };
    let models: Vec<ModelRow> = listed
        .into_iter()
        .map(|m| ModelRow {
            id: m.id,
            output_table_id: m.output_table_id,
            input_table_id: m.input_table_id,
            current_version: m.current_version,
            created_by: m.created_by,
            created_at: m.created_at,
            updated_at: m.updated_at,
        })
        .collect();
    for model_id in deps::dependents_in_order(&models, &row.id) {
        if let Err(e) = rebuild(state, &model_id, "sync").await {
            tracing::warn!("model '{model_id}' did not rebuild after '{source_id}/{table}': {e}");
        }
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

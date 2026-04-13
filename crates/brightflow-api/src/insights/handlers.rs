use axum::{extract::State, Json};
use std::time::Instant;

use brightflow_insights::analysis::engine::AnalysisEngine;
use brightflow_insights::analysis::tree::ReviewCadence;
use brightflow_insights::data::merge::{build_schema, ColumnOverride, TableSettingsOverride};
use brightflow_insights::debug::DebugLog;

use crate::analytics::session::DatasetData;
use crate::insights::types::{InsightsResponse, ReviewRequest, TrendsRequest};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;
use tracing::instrument;

/// Run a review analysis on a dataset
#[instrument(skip(state, req))]
pub async fn run_review(
    State(state): State<AppState>,
    Json(req): Json<ReviewRequest>,
) -> AppResult<Json<InsightsResponse>> {
    let cadence = parse_cadence(&req.cadence)?;
    let (data, overrides, settings) = get_dataset_data_and_overrides(&state, &req.dataset_id)?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();
    let cadence_str = req.cadence.clone();

    let result = tokio::task::spawn_blocking(move || {
        let df = materialize_data(data)?;
        let schema = build_schema(&df, &overrides, settings.as_ref())
            .map_err(|e| AppError::Analysis(format!("Schema build failed: {e}")))?;
        let engine = AnalysisEngine::new(2.0, 0.05, 3);
        engine.run_review_with_cadence(&df, &schema, cadence, &DebugLog::disabled())
    })
    .await??;

    let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let node_count = result.tree.nodes.len();
    let finding_count = result.tree.roots.len();

    let tree_json = serde_json::to_value(&result.tree)
        .map_err(|e| AppError::Internal(format!("Failed to serialize analysis tree: {e}")))?;

    Ok(Json(InsightsResponse {
        dataset_id,
        report_type: format!("review_{cadence_str}"),
        tree: tree_json,
        node_count,
        finding_count,
        first_level_count: result.first_level_count,
        deeper_count: result.deeper_count,
        execution_time_ms,
    }))
}

/// Run a trends analysis on a dataset
#[instrument(skip(state, req))]
pub async fn run_trends(
    State(state): State<AppState>,
    Json(req): Json<TrendsRequest>,
) -> AppResult<Json<InsightsResponse>> {
    let (data, overrides, settings) = get_dataset_data_and_overrides(&state, &req.dataset_id)?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        let df = materialize_data(data)?;
        let schema = build_schema(&df, &overrides, settings.as_ref())
            .map_err(|e| AppError::Analysis(format!("Schema build failed: {e}")))?;
        let engine = AnalysisEngine::new(2.0, 0.05, 3);
        engine.run_trends(&df, &schema)
    })
    .await??;

    let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let node_count = result.tree.nodes.len();
    let finding_count = result.tree.roots.len();

    let tree_json = serde_json::to_value(&result.tree)
        .map_err(|e| AppError::Internal(format!("Failed to serialize analysis tree: {e}")))?;

    Ok(Json(InsightsResponse {
        dataset_id,
        report_type: "trends".to_string(),
        tree: tree_json,
        node_count,
        finding_count,
        first_level_count: result.first_level_count,
        deeper_count: result.deeper_count,
        execution_time_ms,
    }))
}

/// Get DatasetData and overrides for a dataset (never-fail: schema built lazily with DataFrame).
fn get_dataset_data_and_overrides(
    state: &AppState,
    dataset_id: &str,
) -> AppResult<(
    DatasetData,
    Vec<ColumnOverride>,
    Option<TableSettingsOverride>,
)> {
    let dataset = state
        .datasets
        .get_dataset(dataset_id)
        .ok_or_else(|| AppError::NotFound(format!("Dataset '{dataset_id}' not found")))?;

    let data = dataset.data.clone();
    let table_name = dataset.name.clone();
    drop(dataset);

    let overrides = state
        .schema_overrides
        .get(&table_name)
        .map(|v| v.value().clone())
        .unwrap_or_default();

    let settings = state
        .settings_overrides
        .get(&table_name)
        .map(|v| v.value().clone());

    Ok((data, overrides, settings))
}

/// Materialize DatasetData into a DataFrame (safe to call from blocking context)
fn materialize_data(data: DatasetData) -> AppResult<polars::prelude::DataFrame> {
    Ok(match data {
        DatasetData::Uploaded(df) => df,
        DatasetData::Parquet { files } => polars::prelude::LazyFrame::scan_parquet_files(
            files.into(),
            polars::prelude::ScanArgsParquet::default(),
        )?
        .collect()?,
    })
}

/// Parse cadence string to ReviewCadence enum
fn parse_cadence(cadence: &str) -> AppResult<ReviewCadence> {
    match cadence {
        "daily" => Ok(ReviewCadence::Daily),
        "weekly" => Ok(ReviewCadence::Weekly),
        "monthly" => Ok(ReviewCadence::Monthly),
        _ => Err(AppError::BadRequest(format!(
            "Invalid cadence '{cadence}'. Must be 'daily', 'weekly', or 'monthly'."
        ))),
    }
}

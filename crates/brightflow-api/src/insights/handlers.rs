use axum::{extract::State, Json};
use std::path::PathBuf;
use std::time::Instant;

use brightflow_insights::analysis::engine::AnalysisEngine;
use brightflow_insights::analysis::tree::ReviewCadence;
use brightflow_insights::data::merge::{build_schema, ColumnOverride, TableSettingsOverride};
use brightflow_insights::debug::DebugLog;

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
    let (files, table_name, overrides, settings) = resolve_dataset(&state, &req.dataset_id).await?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();
    let cadence_str = req.cadence.clone();

    let result = tokio::task::spawn_blocking(move || {
        let df = scan_parquet_files(files)?;
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

    tracing::info!(
        "Review analysis on '{}' completed in {:.0}ms ({} nodes)",
        table_name,
        execution_time_ms,
        node_count,
    );

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
    let (files, table_name, overrides, settings) = resolve_dataset(&state, &req.dataset_id).await?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        let df = scan_parquet_files(files)?;
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

    tracing::info!(
        "Trends analysis on '{}' completed in {:.0}ms ({} nodes)",
        table_name,
        execution_time_ms,
        node_count,
    );

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

/// Resolve a dataset ID to Parquet file paths, table name, and schema overrides.
///
/// Loads directly from the Parquet store — no in-memory DatasetManager needed.
/// Accepts dataset IDs in `"store:{table_name}"` format or plain table names.
async fn resolve_dataset(
    state: &AppState,
    dataset_id: &str,
) -> AppResult<(
    Vec<PathBuf>,
    String,
    Vec<ColumnOverride>,
    Option<TableSettingsOverride>,
)> {
    let table_name = dataset_id
        .strip_prefix("store:")
        .unwrap_or(dataset_id)
        .to_string();

    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;

    let files = store
        .get_table_parquet_paths(&table_name)
        .await
        .map_err(|e| AppError::NotFound(format!("Table '{table_name}' not found in store: {e}")))?;

    if files.is_empty() {
        return Err(AppError::NotFound(format!(
            "Table '{table_name}' has no data files"
        )));
    }

    let overrides = state
        .schema_overrides
        .get(&table_name)
        .map(|v| v.value().clone())
        .unwrap_or_default();

    let settings = state
        .settings_overrides
        .get(&table_name)
        .map(|v| v.value().clone());

    Ok((files, table_name, overrides, settings))
}

/// Scan Parquet files into a collected DataFrame (safe to call from blocking context)
fn scan_parquet_files(files: Vec<PathBuf>) -> AppResult<polars::prelude::DataFrame> {
    Ok(polars::prelude::LazyFrame::scan_parquet_files(
        files.into(),
        polars::prelude::ScanArgsParquet::default(),
    )?
    .collect()?)
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

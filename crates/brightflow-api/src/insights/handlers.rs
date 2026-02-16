use axum::{extract::State, Json};
use std::time::Instant;

use brightflow_insights::analysis::engine::AnalysisEngine;
use brightflow_insights::analysis::tree::ReviewCadence;
use brightflow_insights::data::schema::DataSchema;
use brightflow_insights::debug::DebugLog;

use crate::insights::types::{InsightsResponse, ReviewRequest, TrendsRequest};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Run a review analysis on a dataset
pub async fn run_review(
    State(state): State<AppState>,
    Json(req): Json<ReviewRequest>,
) -> AppResult<Json<InsightsResponse>> {
    let cadence = parse_cadence(&req.cadence)?;
    let (df, schema) = get_dataset_and_schema(&state, &req.dataset_id)?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();
    let cadence_str = req.cadence.clone();

    let tree = tokio::task::spawn_blocking(move || {
        let engine = AnalysisEngine::new(2.0, 0.05, 3);
        engine.run_review_with_cadence(&df, &schema, cadence, &DebugLog::disabled())
    })
    .await??;

    let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let node_count = tree.nodes.len();
    let finding_count = tree.roots.len();

    let tree_json = serde_json::to_value(&tree)
        .map_err(|e| AppError::Internal(format!("Failed to serialize analysis tree: {e}")))?;

    Ok(Json(InsightsResponse {
        dataset_id,
        report_type: format!("review_{cadence_str}"),
        tree: tree_json,
        node_count,
        finding_count,
        execution_time_ms,
    }))
}

/// Run a trends analysis on a dataset
pub async fn run_trends(
    State(state): State<AppState>,
    Json(req): Json<TrendsRequest>,
) -> AppResult<Json<InsightsResponse>> {
    let (df, schema) = get_dataset_and_schema(&state, &req.dataset_id)?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();

    let tree = tokio::task::spawn_blocking(move || {
        let engine = AnalysisEngine::new(2.0, 0.05, 3);
        engine.run_trends(&df, &schema)
    })
    .await??;

    let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let node_count = tree.nodes.len();
    let finding_count = tree.roots.len();

    let tree_json = serde_json::to_value(&tree)
        .map_err(|e| AppError::Internal(format!("Failed to serialize analysis tree: {e}")))?;

    Ok(Json(InsightsResponse {
        dataset_id,
        report_type: "trends".to_string(),
        tree: tree_json,
        node_count,
        finding_count,
        execution_time_ms,
    }))
}

/// Get DataFrame and schema for a dataset, returning appropriate errors
fn get_dataset_and_schema(
    state: &AppState,
    dataset_id: &str,
) -> AppResult<(polars::prelude::DataFrame, DataSchema)> {
    let dataset = state
        .datasets
        .get_dataset(dataset_id)
        .ok_or_else(|| AppError::NotFound(format!("Dataset '{dataset_id}' not found")))?;

    let df = dataset.df.clone();
    let table_name = dataset.name.clone();
    drop(dataset);

    let schema = state.get_schema(&table_name).ok_or_else(|| {
        AppError::BadRequest(format!(
            "No schema configured for table '{table_name}'. \
             Add a YAML schema file to the schemas/ directory."
        ))
    })?;

    Ok((df, schema))
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

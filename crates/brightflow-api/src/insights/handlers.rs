use axum::{extract::State, Json};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use brightflow_engine::analysis::engine::AnalysisEngine;
use brightflow_engine::analysis::history::{value_signature, HistoryEntry};
use brightflow_engine::analysis::scoring::ScoringContext;
use brightflow_engine::analysis::select::{dimension_of, measure_of};
use brightflow_engine::analysis::tree::{AnalysisTree, ReviewCadence};
use brightflow_engine::data::merge::{build_schema, ColumnOverride, TableSettingsOverride};
use brightflow_engine::data::schema::DataSchema;
use brightflow_engine::debug::DebugLog;

use crate::insights::types::{EngineConfig, InsightsResponse, ReviewRequest, TrendsRequest};
use crate::shared::{AppError, AppResult};
use crate::state::{cache_key, AppState};
use tracing::instrument;

const DEFAULT_Z: f64 = 2.0;
const DEFAULT_P: f64 = 0.05;
const DEFAULT_MIN_EFFECT: f64 = 0.1;
const DEFAULT_MAX_RESULTS: usize = 50;
const DEFAULT_MAX_DEPTH: usize = 3;

/// Build the scoring context from the resolved schema + request config.
/// This is where KPI columns and the min-effect floor actually reach scoring.
fn scoring_context(schema: &DataSchema, config: &EngineConfig) -> ScoringContext {
    ScoringContext::new()
        .with_kpis(schema.kpi_columns.iter().cloned().collect())
        .with_min_effect(config.min_effect_size.unwrap_or(DEFAULT_MIN_EFFECT))
}

fn resolve_config(req: EngineConfig) -> EngineConfig {
    EngineConfig {
        z_threshold: Some(req.z_threshold.unwrap_or(DEFAULT_Z)),
        p_threshold: Some(req.p_threshold.unwrap_or(DEFAULT_P)),
        min_effect_size: Some(req.min_effect_size.unwrap_or(DEFAULT_MIN_EFFECT)),
        select_top: Some(req.select_top.unwrap_or(DEFAULT_MAX_RESULTS)),
        max_depth: Some(req.max_depth.unwrap_or(DEFAULT_MAX_DEPTH)),
    }
}

fn now_epoch() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

/// Persisted curation context around one insights run: prior exposure for
/// novelty decay, plus dismissals/pins/suppressions to apply to the output.
#[derive(Default)]
struct InsightCuration {
    table_id: Option<String>,
    history: HashMap<String, HistoryEntry>,
    dismissed: HashSet<String>,
    pinned: HashSet<String>,
    suppressed_columns: HashSet<String>,
    suppressed_segments: HashSet<String>,
}

/// Load curation state from the store. Failures degrade to "no curation" —
/// an unreadable history must never block an insights run.
async fn load_curation(state: &AppState, source_id: &str, table_name: &str) -> InsightCuration {
    let mut out = InsightCuration::default();
    let Some(store) = state.store() else {
        return out;
    };
    let Ok(Some(table)) = store.db().get_table(source_id, table_name).await else {
        return out;
    };
    let table_id = table.id.clone();
    if let Ok(rows) = store.db().get_insight_history(&table_id).await {
        for row in rows {
            out.history.insert(
                row.fingerprint,
                HistoryEntry {
                    shown_count: u32::try_from(row.shown_count.max(0)).unwrap_or(u32::MAX),
                    last_shown_epoch: row.last_shown_at,
                    last_value_sig: row.last_value_sig,
                },
            );
        }
    }
    if let Ok(rows) = store.db().get_insight_states(&table_id).await {
        for row in rows {
            match row.state.as_str() {
                "dismissed" => {
                    out.dismissed.insert(row.fingerprint);
                },
                "pinned" => {
                    out.pinned.insert(row.fingerprint);
                },
                _ => {},
            }
        }
    }
    if let Ok(rows) = store.db().get_insight_suppressions(&table_id).await {
        for row in rows {
            match row.kind.as_str() {
                "column" => {
                    out.suppressed_columns.insert(row.target);
                },
                "segment" => {
                    out.suppressed_segments.insert(row.target);
                },
                _ => {},
            }
        }
    }
    out.table_id = Some(table_id);
    out
}

/// Drop dismissed/suppressed roots and float pinned ones to the top.
fn apply_curation(tree: &mut AnalysisTree, curation: &InsightCuration) {
    if curation.dismissed.is_empty()
        && curation.pinned.is_empty()
        && curation.suppressed_columns.is_empty()
        && curation.suppressed_segments.is_empty()
    {
        return;
    }
    let mut kept: Vec<_> = tree
        .roots
        .iter()
        .copied()
        .filter(|id| {
            let Some(node) = tree.nodes.get(id.0) else {
                return false;
            };
            if !node.fingerprint.is_empty() && curation.dismissed.contains(&node.fingerprint) {
                return false;
            }
            if curation.suppressed_columns.contains(&measure_of(node)) {
                return false;
            }
            if let Some(dim) = dimension_of(node) {
                if curation.suppressed_segments.contains(&dim) {
                    return false;
                }
            }
            true
        })
        .collect();
    kept.sort_by_key(|id| {
        let pinned = tree
            .nodes
            .get(id.0)
            .is_some_and(|n| curation.pinned.contains(&n.fingerprint));
        (
            !pinned,
            tree.nodes
                .get(id.0)
                .and_then(|n| n.rank)
                .unwrap_or(u32::MAX),
        )
    });
    tree.roots = kept;
}

/// Fire-and-forget write-back of shown roots into insight_history.
fn record_shown_insights(state: &AppState, table_id: Option<String>, tree: &AnalysisTree) {
    let Some(table_id) = table_id else {
        return;
    };
    let Some(store) = state.store().cloned() else {
        return;
    };
    let shown: Vec<(String, String, String, String)> = tree
        .roots
        .iter()
        .filter_map(|id| tree.nodes.get(id.0))
        .filter(|n| !n.fingerprint.is_empty())
        .map(|n| {
            (
                n.fingerprint.clone(),
                n.summary.clone(),
                n.analysis.kind_name().to_string(),
                value_signature(n),
            )
        })
        .collect();
    if shown.is_empty() {
        return;
    }
    let now = now_epoch();
    tokio::spawn(async move {
        if let Err(e) = store
            .db()
            .record_shown_insights(&table_id, &shown, now)
            .await
        {
            tracing::warn!("failed to record shown insights: {e}");
        }
    });
}

/// Run a review analysis on a dataset
#[instrument(skip(state, req))]
pub async fn run_review(
    State(state): State<AppState>,
    Json(req): Json<ReviewRequest>,
) -> AppResult<Json<InsightsResponse>> {
    let cadence = parse_cadence(&req.cadence)?;
    let (files, table_name, overrides, settings) =
        resolve_dataset(&state, &req.source_id, &req.dataset_id).await?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();
    let cadence_str = req.cadence.clone();
    let config = resolve_config(req.config);
    let config_for_engine = config.clone();

    let curation = load_curation(&state, &req.source_id, &table_name).await;
    let history = curation.history.clone();

    let mut result = tokio::task::spawn_blocking(move || {
        let df = scan_parquet_files(files)?;
        let schema = build_schema(&df, &overrides, settings.as_ref())
            .map_err(|e| AppError::Analysis(format!("Schema build failed: {e}")))?;
        let engine = AnalysisEngine::new(
            config_for_engine.z_threshold.unwrap_or(DEFAULT_Z),
            config_for_engine.p_threshold.unwrap_or(DEFAULT_P),
            config_for_engine.max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        )
        .with_scoring_ctx(scoring_context(&schema, &config_for_engine))
        .with_select_top(config_for_engine.select_top.unwrap_or(DEFAULT_MAX_RESULTS))
        .with_history(history, now_epoch());
        engine.run_review_with_cadence(&df, &schema, cadence, &DebugLog::disabled())
    })
    .await??;

    apply_curation(&mut result.tree, &curation);
    record_shown_insights(&state, curation.table_id, &result.tree);

    let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let node_count = result.tree.nodes.len();
    let finding_count = result.tree.roots.len();
    let total_candidates = result.first_level_count + result.deeper_count;

    tracing::info!(
        "Review analysis on '{}' completed in {:.0}ms ({} nodes)",
        table_name,
        execution_time_ms,
        node_count,
    );

    Ok(Json(InsightsResponse {
        dataset_id,
        report_type: format!("review_{cadence_str}"),
        tree: result.tree,
        node_count,
        finding_count,
        first_level_count: result.first_level_count,
        deeper_count: result.deeper_count,
        execution_time_ms,
        total_candidates,
        config_used: config,
    }))
}

/// Run a trends analysis on a dataset
#[instrument(skip(state, req))]
pub async fn run_trends(
    State(state): State<AppState>,
    Json(req): Json<TrendsRequest>,
) -> AppResult<Json<InsightsResponse>> {
    let (files, table_name, overrides, settings) =
        resolve_dataset(&state, &req.source_id, &req.dataset_id).await?;

    let start = Instant::now();
    let dataset_id = req.dataset_id.clone();
    let config = resolve_config(req.config);
    let config_for_engine = config.clone();

    let curation = load_curation(&state, &req.source_id, &table_name).await;
    let history = curation.history.clone();

    let mut result = tokio::task::spawn_blocking(move || {
        let df = scan_parquet_files(files)?;
        let schema = build_schema(&df, &overrides, settings.as_ref())
            .map_err(|e| AppError::Analysis(format!("Schema build failed: {e}")))?;
        let engine = AnalysisEngine::new(
            config_for_engine.z_threshold.unwrap_or(DEFAULT_Z),
            config_for_engine.p_threshold.unwrap_or(DEFAULT_P),
            config_for_engine.max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        )
        .with_scoring_ctx(scoring_context(&schema, &config_for_engine))
        .with_select_top(config_for_engine.select_top.unwrap_or(DEFAULT_MAX_RESULTS))
        .with_history(history, now_epoch());
        engine.run_trends(&df, &schema)
    })
    .await??;

    apply_curation(&mut result.tree, &curation);
    record_shown_insights(&state, curation.table_id, &result.tree);

    let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let node_count = result.tree.nodes.len();
    let finding_count = result.tree.roots.len();
    let total_candidates = result.first_level_count + result.deeper_count;

    tracing::info!(
        "Trends analysis on '{}' completed in {:.0}ms ({} nodes)",
        table_name,
        execution_time_ms,
        node_count,
    );

    Ok(Json(InsightsResponse {
        dataset_id,
        report_type: "trends".to_string(),
        tree: result.tree,
        node_count,
        finding_count,
        first_level_count: result.first_level_count,
        deeper_count: result.deeper_count,
        execution_time_ms,
        total_candidates,
        config_used: config,
    }))
}

/// Resolve a dataset ID to Parquet file paths, table name, and schema overrides.
///
/// Loads directly from the Parquet store — no in-memory DatasetManager needed.
/// Accepts dataset IDs in `"store:{table_name}"` format or plain table names.
async fn resolve_dataset(
    state: &AppState,
    source_id: &str,
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
        .get_table_parquet_paths(source_id, &table_name)
        .await
        .map_err(|e| AppError::NotFound(format!("Table '{table_name}' not found in store: {e}")))?;

    if files.is_empty() {
        return Err(AppError::NotFound(format!(
            "Table '{table_name}' has no data files"
        )));
    }

    let key = cache_key(source_id, &table_name);
    let overrides = state
        .schema_overrides
        .get(&key)
        .map(|v| v.value().clone())
        .unwrap_or_default();

    let settings = state
        .settings_overrides
        .get(&key)
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

/// `GET /api/sources/{source_id}/tables/{table}/insights/history`
pub async fn get_history(
    State(state): State<AppState>,
    axum::extract::Path((source_id, table)): axum::extract::Path<(String, String)>,
) -> AppResult<Json<Vec<brightflow_store::InsightHistoryRow>>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{table}' not found")))?;
    let rows = store.db().get_insight_history(&table_row.id).await?;
    Ok(Json(rows))
}

/// `DELETE /api/sources/{source_id}/tables/{table}/insights/history`
pub async fn reset_history(
    State(state): State<AppState>,
    axum::extract::Path((source_id, table)): axum::extract::Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{table}' not found")))?;
    let deleted = store.db().reset_insight_history(&table_row.id).await?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

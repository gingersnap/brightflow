use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use brightflow_engine::analysis::engine::AnalysisEngine;
use brightflow_engine::analysis::history::{value_signature, HistoryEntry};
use brightflow_engine::analysis::polarity::apply_sentiment;
use brightflow_engine::analysis::scoring::ScoringContext;
use brightflow_engine::analysis::select::{dimension_of, measure_of};
use brightflow_engine::analysis::tree::{AnalysisTree, ReportType, ReviewCadence};
use brightflow_engine::data::merge::{build_schema, ColumnOverride, TableSettingsOverride};
use brightflow_engine::data::schema::DataSchema;
use brightflow_engine::debug::DebugLog;

use crate::insights::types::{
    DriversRequest, EngineConfig, InsightRunResponse, InsightsResponse, ReviewRequest,
    TrendsRequest,
};
use crate::shared::{AppError, AppResult};
use crate::state::{cache_key, AppState};
use tracing::instrument;

const DEFAULT_Z: f64 = 2.0;
const DEFAULT_P: f64 = 0.05;
const DEFAULT_MIN_EFFECT: f64 = 0.1;
const DEFAULT_MAX_RESULTS: usize = 50;
const DEFAULT_MAX_DEPTH: usize = 3;

/// Who asked for this run. Post-sync runs deliberately do NOT count as
/// "shown" — novelty history only records what a human actually saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTrigger {
    Manual,
    PostSync,
}

impl RunTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::PostSync => "post_sync",
        }
    }
}

/// Which report to compute (cadence travels with review).
#[derive(Debug, Clone)]
pub enum ReportKind {
    Review { cadence: ReviewCadence },
    Trends,
    Drivers,
}

impl ReportKind {
    fn report_type_string(&self) -> String {
        match self {
            Self::Review { cadence } => format!("review_{}", cadence.suffix()),
            Self::Trends => "trends".to_string(),
            Self::Drivers => "drivers".to_string(),
        }
    }
}

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
            .map_or(0, |d| d.as_secs()),
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

/// Roots whose fingerprints have never been recorded as shown before.
fn count_new_findings(tree: &AnalysisTree, history: &HashMap<String, HistoryEntry>) -> usize {
    tree.roots
        .iter()
        .filter_map(|id| tree.nodes.get(id.0))
        .filter(|n| !n.fingerprint.is_empty() && !history.contains_key(&n.fingerprint))
        .count()
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

/// Fire-and-forget: persist an `insight_runs` row and push the
/// `insightsComputed` WS event.
#[allow(clippy::too_many_arguments)]
fn record_insight_run(
    state: &AppState,
    table_id: Option<String>,
    source_id: String,
    table_name: String,
    report_type: String,
    trigger: RunTrigger,
    finding_count: usize,
    new_finding_count: usize,
    top_summary: Option<String>,
    execution_time_ms: f64,
) {
    let Some(table_id) = table_id else {
        return;
    };
    let Some(store) = state.store().cloned() else {
        return;
    };
    let state = state.clone();
    tokio::spawn(async move {
        let inserted = store
            .db()
            .insert_insight_run(
                &table_id,
                &source_id,
                &table_name,
                &report_type,
                trigger.as_str(),
                i64::try_from(finding_count).unwrap_or(i64::MAX),
                i64::try_from(new_finding_count).unwrap_or(i64::MAX),
                top_summary.as_deref(),
                execution_time_ms,
                now_epoch(),
            )
            .await;
        match inserted {
            Ok(row) => crate::actions::events::emit_insights_computed(&state, &row),
            Err(e) => tracing::warn!("failed to record insight run: {e}"),
        }
    });
}

/// Shared core for every insights run — manual handlers and the post-sync
/// auto-runner both come through here.
pub(crate) async fn run_report_core(
    state: &AppState,
    source_id: &str,
    dataset_id: &str,
    config: EngineConfig,
    kind: ReportKind,
    trigger: RunTrigger,
) -> AppResult<InsightsResponse> {
    let (files, table_name, overrides, settings) =
        resolve_dataset(state, source_id, dataset_id).await?;

    let start = Instant::now();
    let config = resolve_config(config);
    let config_for_engine = config.clone();
    let kind_for_engine = kind.clone();

    let curation = load_curation(state, source_id, &table_name).await;
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
        let mut result = match kind_for_engine {
            ReportKind::Review { cadence } => {
                engine.run_review_with_cadence(&df, &schema, cadence, &DebugLog::disabled())?
            },
            ReportKind::Trends => engine.run_trends(&df, &schema)?,
            ReportKind::Drivers => {
                engine.run_report(&df, &schema, ReportType::Drivers, &DebugLog::disabled())?
            },
        };
        // Display-only sentiment tags from measure polarity.
        apply_sentiment(&mut result.tree, &schema.polarity);
        Ok::<_, AppError>(result)
    })
    .await??;

    apply_curation(&mut result.tree, &curation);
    let new_finding_count = count_new_findings(&result.tree, &curation.history);
    // "Shown" means shown to a human — auto-runs must not decay novelty.
    if trigger == RunTrigger::Manual {
        record_shown_insights(state, curation.table_id.clone(), &result.tree);
    }

    let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let node_count = result.tree.nodes.len();
    let finding_count = result.tree.roots.len();
    let total_candidates = result.first_level_count + result.deeper_count;
    let report_type = kind.report_type_string();

    let top_summary = result
        .tree
        .roots
        .first()
        .and_then(|id| result.tree.nodes.get(id.0))
        .map(|n| n.summary.clone());
    record_insight_run(
        state,
        curation.table_id,
        source_id.to_string(),
        table_name.clone(),
        report_type.clone(),
        trigger,
        finding_count,
        new_finding_count,
        top_summary,
        execution_time_ms,
    );

    tracing::info!(
        "{} analysis on '{}' completed in {:.0}ms ({} nodes, {} new)",
        report_type,
        table_name,
        execution_time_ms,
        node_count,
        new_finding_count,
    );

    Ok(InsightsResponse {
        dataset_id: dataset_id.to_string(),
        report_type,
        tree: result.tree,
        node_count,
        finding_count,
        first_level_count: result.first_level_count,
        deeper_count: result.deeper_count,
        execution_time_ms,
        total_candidates,
        config_used: config,
    })
}

pub(crate) async fn run_review_core(
    state: &AppState,
    req: ReviewRequest,
    trigger: RunTrigger,
) -> AppResult<InsightsResponse> {
    let cadence = parse_cadence(&req.cadence)?;
    run_report_core(
        state,
        &req.source_id,
        &req.dataset_id,
        req.config,
        ReportKind::Review { cadence },
        trigger,
    )
    .await
}

pub(crate) async fn run_trends_core(
    state: &AppState,
    req: TrendsRequest,
    trigger: RunTrigger,
) -> AppResult<InsightsResponse> {
    run_report_core(
        state,
        &req.source_id,
        &req.dataset_id,
        req.config,
        ReportKind::Trends,
        trigger,
    )
    .await
}

pub(crate) async fn run_drivers_core(
    state: &AppState,
    req: DriversRequest,
    trigger: RunTrigger,
) -> AppResult<InsightsResponse> {
    run_report_core(
        state,
        &req.source_id,
        &req.dataset_id,
        req.config,
        ReportKind::Drivers,
        trigger,
    )
    .await
}

/// Run a review analysis on a dataset
#[instrument(skip(state, req))]
pub async fn run_review(
    State(state): State<AppState>,
    Json(req): Json<ReviewRequest>,
) -> AppResult<Json<InsightsResponse>> {
    Ok(Json(
        run_review_core(&state, req, RunTrigger::Manual).await?,
    ))
}

/// Run a trends analysis on a dataset
#[instrument(skip(state, req))]
pub async fn run_trends(
    State(state): State<AppState>,
    Json(req): Json<TrendsRequest>,
) -> AppResult<Json<InsightsResponse>> {
    Ok(Json(
        run_trends_core(&state, req, RunTrigger::Manual).await?,
    ))
}

/// Run a drivers analysis on a dataset
#[instrument(skip(state, req))]
pub async fn run_drivers(
    State(state): State<AppState>,
    Json(req): Json<DriversRequest>,
) -> AppResult<Json<InsightsResponse>> {
    Ok(Json(
        run_drivers_core(&state, req, RunTrigger::Manual).await?,
    ))
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
    Path((source_id, table)): Path<(String, String)>,
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
    Path((source_id, table)): Path<(String, String)>,
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

#[derive(Debug, serde::Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
}

/// `GET /api/sources/{source_id}/tables/{table}/insights/runs?limit=`
pub async fn get_runs(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
    Query(query): Query<RunsQuery>,
) -> AppResult<Json<Vec<InsightRunResponse>>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{table}' not found")))?;
    let limit = query.limit.unwrap_or(25).clamp(1, 100);
    let rows = store.db().list_insight_runs(&table_row.id, limit).await?;
    Ok(Json(
        rows.into_iter().map(InsightRunResponse::from_row).collect(),
    ))
}

/// `GET /api/sources/{source_id}/insights/latest` — latest run per table,
/// for badge hydration on load/reconnect.
pub async fn get_latest_runs(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> AppResult<Json<Vec<InsightRunResponse>>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let rows = store
        .db()
        .latest_insight_runs_for_source(&source_id)
        .await?;
    Ok(Json(
        rows.into_iter().map(InsightRunResponse::from_row).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use brightflow_engine::analysis::tree::AnalysisType;

    fn seg_analysis(target: &str, dim: &str) -> AnalysisType {
        AnalysisType::Segment {
            target_column: target.to_string(),
            segment_column: dim.to_string(),
            segment_value: "x".to_string(),
            contribution: 1.0,
            change_percent: 10.0,
            contribution_pct: 50.0,
            p_value: 0.01,
        }
    }

    fn tree_with_roots(specs: &[(&str, AnalysisType, f64)]) -> AnalysisTree {
        let mut tree = AnalysisTree::new();
        for (i, (fp, analysis, score)) in specs.iter().enumerate() {
            let id = tree.add_root(analysis.clone(), *score, String::new());
            if let Some(n) = tree.nodes.get_mut(id.0) {
                n.fingerprint = (*fp).to_string();
                n.rank = Some(u32::try_from(i).unwrap() + 1);
            }
        }
        tree
    }

    fn trend_analysis(col: &str) -> AnalysisType {
        AnalysisType::Trend {
            column: col.to_string(),
            direction: brightflow_engine::analysis::tree::TrendDirection::Increasing,
            slope: 1.0,
            r_squared: 0.9,
            p_value: 0.001,
        }
    }

    #[test]
    fn apply_curation_drops_dismissed_roots() {
        let mut tree = tree_with_roots(&[
            ("fp1", trend_analysis("a"), 0.9),
            ("fp2", trend_analysis("b"), 0.8),
        ]);
        let curation = InsightCuration {
            dismissed: HashSet::from(["fp1".to_string()]),
            ..Default::default()
        };
        apply_curation(&mut tree, &curation);
        assert_eq!(tree.roots.len(), 1);
        assert_eq!(tree.nodes[tree.roots[0].0].fingerprint, "fp2");
    }

    #[test]
    fn apply_curation_floats_pinned_roots() {
        let mut tree = tree_with_roots(&[
            ("fp1", trend_analysis("a"), 0.9),
            ("fp2", trend_analysis("b"), 0.8),
            ("fp3", trend_analysis("c"), 0.7),
        ]);
        let curation = InsightCuration {
            pinned: HashSet::from(["fp3".to_string()]),
            ..Default::default()
        };
        apply_curation(&mut tree, &curation);
        assert_eq!(tree.nodes[tree.roots[0].0].fingerprint, "fp3");
        assert_eq!(tree.nodes[tree.roots[1].0].fingerprint, "fp1");
    }

    #[test]
    fn apply_curation_suppresses_by_measure_and_dimension() {
        let mut tree = tree_with_roots(&[
            ("fp1", trend_analysis("revenue"), 0.9),
            ("fp2", seg_analysis("orders", "region"), 0.8),
            ("fp3", trend_analysis("orders"), 0.7),
        ]);
        let curation = InsightCuration {
            suppressed_columns: HashSet::from(["revenue".to_string()]),
            suppressed_segments: HashSet::from(["region".to_string()]),
            ..Default::default()
        };
        apply_curation(&mut tree, &curation);
        assert_eq!(tree.roots.len(), 1);
        assert_eq!(tree.nodes[tree.roots[0].0].fingerprint, "fp3");
    }

    #[test]
    fn new_findings_counts_unseen_fingerprints_only() {
        let tree = tree_with_roots(&[
            ("seen", trend_analysis("a"), 0.9),
            ("fresh", trend_analysis("b"), 0.8),
            ("", trend_analysis("c"), 0.7),
        ]);
        let mut history = HashMap::new();
        history.insert(
            "seen".to_string(),
            HistoryEntry {
                shown_count: 3,
                last_shown_epoch: 0,
                last_value_sig: String::new(),
            },
        );
        // "fresh" is new; the empty fingerprint never counts.
        assert_eq!(count_new_findings(&tree, &history), 1);
    }

    #[test]
    fn parse_cadence_accepts_known_values_only() {
        assert!(parse_cadence("daily").is_ok());
        assert!(parse_cadence("weekly").is_ok());
        assert!(parse_cadence("monthly").is_ok());
        assert!(parse_cadence("hourly").is_err());
    }
}

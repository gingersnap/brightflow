//! Enrichment-function endpoints: CRUD + versions + lifecycle, synchronous
//! sample runs, token estimates, and async full/incremental runs.
//!
//! This file is HTTP wiring over two siblings: the pure spec rules live in
//! `validate` (unit-tested there), the async run loop in `runner`.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use brightflow_engine::enrichment::FunctionSpec;

use super::runner::{output_columns_for, spec_hash_for, RunSpec};
use super::vocab;
use brightflow_store::{EnrichmentFunctionRow, ParquetStore, TableRow};

use crate::enrichment::runner;
use crate::enrichment::types::{
    CreateFunctionRequest, EnrichFunctionResponse, EnrichRunResponse, EstimateResponse,
    FunctionVersionResponse, SampleCellResponse, SampleRunRequest, SampleRunResponse,
    StartEnrichRunRequest, StartEnrichRunResponse, UpdateFunctionRequest,
};
use crate::enrichment::validate::{parse_spec, valid_name, validate_spec};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

const MAX_SAMPLE_ROWS: usize = 100;
const DEFAULT_SAMPLE_ROWS: usize = 10;
/// Heuristic fallback when the cache has no token history: ~4 chars/token on
/// the prompt plus a per-output completion allowance.
const HEURISTIC_COMPLETION_TOKENS_PER_OUTPUT: i64 = 150;

fn store(state: &AppState) -> AppResult<&std::sync::Arc<ParquetStore>> {
    state.require_store()
}

async fn to_response(
    store: &ParquetStore,
    row: &EnrichmentFunctionRow,
) -> AppResult<EnrichFunctionResponse> {
    let version = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?
        .ok_or_else(|| {
            AppError::Internal(format!("function {} has no version snapshot", row.id))
        })?;
    let config: serde_json::Value =
        serde_json::from_str(&version.config_json).unwrap_or(serde_json::Value::Null);
    let active_run_id = store
        .db()
        .active_enrichment_run(&row.id)
        .await?
        .map(|r| r.id);

    // Approximate staleness: table rows minus cells cached under the current
    // spec. Duplicated-content rows make this an over-count — acceptable for
    // a badge.
    let stale_row_count = match serde_json::from_str::<FunctionSpec>(&version.config_json)
        .ok()
        .as_ref()
        .and_then(spec_hash_for)
    {
        Some(shash) => {
            let (cached, _errors) = store.db().count_cached(&row.id, &shash).await?;
            let total = store
                .db()
                .get_table_by_id(&row.table_id)
                .await?
                .map_or(0, |t| t.total_rows);
            Some((total - cached).max(0))
        },
        None => None,
    };

    Ok(EnrichFunctionResponse {
        id: row.id.clone(),
        name: row.name.clone(),
        kind: row.kind.clone(),
        status: row.status.clone(),
        version: row.current_version,
        config,
        stale_row_count,
        active_run_id,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

/// Resolve a function id to (function row, owning table row).
async fn function_context(
    store: &ParquetStore,
    id: &str,
) -> AppResult<(EnrichmentFunctionRow, TableRow)> {
    let row = store
        .db()
        .get_enrichment_function(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("function {id} not found")))?;
    let table = store
        .db()
        .get_table_by_id(&row.table_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table for function {id} not found")))?;
    Ok((row, table))
}

/// Parse a stored config. The spec's own kind tag decides what it is.
fn spec_from_config(config_json: &str) -> AppResult<FunctionSpec> {
    serde_json::from_str::<FunctionSpec>(config_json)
        .map_err(|e| AppError::Internal(format!("stored config unreadable: {e}")))
}

/// Load the runnable form of a function's current version.
async fn run_spec_from_row(
    store: &ParquetStore,
    row: &EnrichmentFunctionRow,
) -> AppResult<RunSpec> {
    let version = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?
        .ok_or_else(|| AppError::Internal("missing version snapshot".to_string()))?;
    let spec = spec_from_config(&version.config_json)?;
    vocab::run_spec_for(store, &row.table_id, spec).await
}

/// `POST /api/sources/{source_id}/tables/{table}/functions`
pub async fn create_function(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
    Json(req): Json<CreateFunctionRequest>,
) -> AppResult<Json<EnrichFunctionResponse>> {
    let store = store(&state)?;
    if !matches!(req.kind.as_str(), "ticket_classify" | "ticket_extract") {
        return Err(AppError::BadRequest(format!(
            "unknown function kind '{}'",
            req.kind
        )));
    }
    if !valid_name(&req.name) {
        return Err(AppError::BadRequest(format!(
            "'{}' is not a legal function name (letters, digits, _; must not start with a digit)",
            req.name
        )));
    }
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table '{table}' not found")))?;
    let columns = table_row.column_names();
    if columns.iter().any(|c| c == &req.name) {
        return Err(AppError::BadRequest(format!(
            "function name '{}' collides with an existing table column",
            req.name
        )));
    }

    let mut spec = parse_spec(&req.kind, &req.config)?;
    validate_spec(&spec, &columns)?;
    // Built-in kinds are promoted from birth: there is no draft state whose
    // materialised columns a sync could wipe, and the vocabulary snapshot is
    // the server's to write, never the client's.
    vocab::inject(store, &table_row.id, &mut spec).await?;
    let status = "promoted";
    let config_json = serde_json::to_string(&spec).map_err(AppError::Json)?;

    let created = store
        .db()
        .create_enrichment_function(&table_row.id, &req.name, &req.kind, status, &config_json)
        .await
        .map_err(|e| {
            if e.is_unique_violation() {
                AppError::Conflict(format!(
                    "a function named '{}' already exists on this table",
                    req.name
                ))
            } else {
                AppError::Store(e)
            }
        })?;
    Ok(Json(to_response(store, &created).await?))
}

/// `GET /api/sources/{source_id}/tables/{table}/functions`
pub async fn list_functions(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
) -> AppResult<Json<Vec<EnrichFunctionResponse>>> {
    let store = store(&state)?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table '{table}' not found")))?;
    let rows = store.db().list_enrichment_functions(&table_row.id).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(to_response(store, row).await?);
    }
    Ok(Json(out))
}

/// `GET /api/functions/{id}`
pub async fn get_function(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<EnrichFunctionResponse>> {
    let store = store(&state)?;
    let (row, _table) = function_context(store, &id).await?;
    Ok(Json(to_response(store, &row).await?))
}

/// `PUT /api/functions/{id}` — new version; optional rerun.
pub async fn update_function(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateFunctionRequest>,
) -> AppResult<Json<EnrichFunctionResponse>> {
    let store = store(&state)?;
    let (row, table_row) = function_context(store, &id).await?;
    let rerun = req.rerun.as_deref().unwrap_or("none");
    if !matches!(rerun, "none" | "all" | "missing") {
        return Err(AppError::BadRequest(format!(
            "unknown rerun mode '{rerun}' (none | all | missing)"
        )));
    }

    let mut spec = parse_spec(&row.kind, &req.config)?;
    let columns = table_row.column_names();
    // The function's own materialized outputs may already be table columns;
    // exclude them from the collision check on edit.
    let previous = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?;
    let own_outputs: Vec<String> = previous
        .as_ref()
        .and_then(|v| serde_json::from_str::<FunctionSpec>(&v.config_json).ok())
        .map(|old| output_columns_for(&old))
        .unwrap_or_default();
    let mut status_col = own_outputs.clone();
    status_col.push(format!("{}__status", row.name));
    let filtered: Vec<String> = columns
        .into_iter()
        .filter(|c| !status_col.contains(c))
        .collect();
    validate_spec(&spec, &filtered)?;
    vocab::inject(store, &table_row.id, &mut spec).await?;

    let config_json = serde_json::to_string(&spec).map_err(AppError::Json)?;
    store
        .db()
        .update_enrichment_function_config(&row.id, &config_json)
        .await?;

    // Cache housekeeping: keep only the new and previous spec's cells.
    if let Some(new_hash) = spec_hash_for(&spec) {
        let mut keep = vec![new_hash];
        if let Some(prev_hash) = previous
            .as_ref()
            .and_then(|v| serde_json::from_str::<FunctionSpec>(&v.config_json).ok())
            .as_ref()
            .and_then(spec_hash_for)
        {
            keep.push(prev_hash);
        }
        keep.dedup();
        store.db().prune_cache_except(&row.id, &keep).await?;
    }

    let updated = store
        .db()
        .get_enrichment_function(&row.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("function {id} not found")))?;

    if rerun != "none" && spec_hash_for(&spec).is_some() {
        // Best effort: a conflicting active run leaves the edit saved.
        if let Err(e) = start_run_internal(&state, &updated, rerun == "all").await {
            tracing::warn!("post-edit rerun not started: {e}");
        }
    }

    Ok(Json(to_response(store, &updated).await?))
}

#[derive(Debug, Deserialize)]
pub struct DeleteFunctionQuery {
    #[serde(default)]
    pub drop_columns: Option<bool>,
}

/// `DELETE /api/functions/{id}?drop_columns=true`
pub async fn delete_function(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DeleteFunctionQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let store = store(&state)?;
    let (row, table_row) = function_context(store, &id).await?;
    if let Some(active) = store.db().active_enrichment_run(&row.id).await? {
        return Err(AppError::Conflict(format!(
            "run {} is active for this function — cancel it first",
            active.id
        )));
    }

    if q.drop_columns.unwrap_or(false) {
        drop_output_columns(store, &row, &table_row).await?;
    }

    store.db().delete_enrichment_function(&row.id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Drop the function's materialised columns (outputs plus `{fn}__status`)
/// from its table, along with the semantics rows the run wrote for them, so
/// a column that no longer exists cannot linger as a label in Explore.
/// Returns how many were present. A no-op for kinds this runner does not
/// materialise.
async fn drop_output_columns(
    store: &ParquetStore,
    row: &EnrichmentFunctionRow,
    table_row: &TableRow,
) -> AppResult<usize> {
    if !super::RUNNABLE_KINDS.contains(&row.kind.as_str()) {
        return Ok(0);
    }
    let Some(version) = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?
    else {
        return Ok(0);
    };
    let Ok(spec) = spec_from_config(&version.config_json) else {
        return Ok(0);
    };
    let mut names: Vec<String> = output_columns_for(&spec);
    names.push(format!("{}__status", row.name));
    let mut out_df = store
        .read_table(&table_row.source_id, &table_row.name)
        .await?;
    let mut dropped = 0_usize;
    for name in &names {
        if out_df.column(name).is_ok() {
            out_df = out_df.drop(name).map_err(AppError::Polars)?;
            dropped += 1;
        }
    }
    if dropped > 0 {
        store
            .replace_table_data(&table_row.source_id, &table_row.name, out_df, None)
            .await?;
        let prov = brightflow_types::Provenance::declared(format!("enrichment:{}", row.name));
        for name in &names {
            store
                .db()
                .delete_column_opinion(&table_row.id, name, &prov)
                .await?;
        }
    }
    Ok(dropped)
}

/// `POST /api/functions/{id}/reset` — forget everything the function computed.
///
/// The configuration stays; every cached cell under every spec hash and the
/// materialised columns go, so the next run starts from nothing. Not
/// undoable, which is why the caller confirms; refused while a run is active.
pub async fn reset_function(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let store = store(&state)?;
    let (row, table_row) = function_context(store, &id).await?;
    if let Some(active) = store.db().active_enrichment_run(&row.id).await? {
        return Err(AppError::Conflict(format!(
            "run {} is active for this function — cancel it first",
            active.id
        )));
    }
    let columns_dropped = drop_output_columns(store, &row, &table_row).await?;
    let cells_deleted = store.db().prune_cache_except(&row.id, &[]).await?;
    Ok(Json(serde_json::json!({
        "cellsDeleted": cells_deleted,
        "columnsDropped": columns_dropped,
    })))
}

/// `GET /api/functions/{id}/versions`
pub async fn list_versions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<FunctionVersionResponse>>> {
    let store = store(&state)?;
    let (row, _) = function_context(store, &id).await?;
    let versions = store
        .db()
        .list_enrichment_function_versions(&row.id)
        .await?;
    Ok(Json(
        versions
            .into_iter()
            .map(|v| FunctionVersionResponse {
                version: v.version,
                config: serde_json::from_str(&v.config_json).unwrap_or(serde_json::Value::Null),
                created_at: v.created_at,
            })
            .collect(),
    ))
}

/// `POST /api/functions/{id}/sample-run` — synchronous, capped at 100 rows.
/// Accepts an unsaved draft config; the content-keyed cache makes draft
/// iteration recompute only what changed.
pub async fn sample_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SampleRunRequest>,
) -> AppResult<Json<SampleRunResponse>> {
    let store = store(&state)?;
    let (row, table_row) = function_context(store, &id).await?;
    if !super::RUNNABLE_KINDS.contains(&row.kind.as_str()) {
        return Err(AppError::BadRequest(
            "sample runs are only available for ticket functions".to_string(),
        ));
    }

    let spec = if let Some(draft) = &req.config {
        let mut parsed = parse_spec(&row.kind, draft)?;
        let columns = table_row.column_names();
        let mut own: Vec<String> = output_columns_for(&parsed);
        own.push(format!("{}__status", row.name));
        let filtered: Vec<String> = columns.into_iter().filter(|c| !own.contains(c)).collect();
        validate_spec(&parsed, &filtered)?;
        vocab::inject(store, &table_row.id, &mut parsed).await?;
        vocab::run_spec_for(store, &table_row.id, parsed).await?
    } else {
        run_spec_from_row(store, &row).await?
    };

    let (provider_id, model) = spec.provider();
    let client = crate::llm::client_for(&state, provider_id, model).await?;
    let limit = req
        .limit
        .unwrap_or(DEFAULT_SAMPLE_ROWS)
        .clamp(1, MAX_SAMPLE_ROWS);
    let df = store
        .read_table(&table_row.source_id, &table_row.name)
        .await?;
    let sample = df.head(Some(limit));
    let inputs = runner::prepare_run_inputs(&sample, &spec)?;

    let outcome = runner::execute_cells(
        store,
        &client,
        &row.id,
        row.current_version,
        &spec,
        &inputs,
        None,
        // A sample is always computed fresh. Reading the cache would replay
        // stale answers precisely when the point is to see what the current
        // prompt produces; writing it would store results from a draft config.
        runner::CacheMode::Bypass,
    )
    .await?;

    let rows: Vec<SampleCellResponse> = inputs
        .iter()
        .filter_map(|input| {
            outcome
                .cells
                .get(&input.hash)
                .map(|cell| SampleCellResponse {
                    row_key: input.hash.clone(),
                    inputs: input.rendered.clone(),
                    value: cell.value.as_ref().map(|v| spec.display_value(v)),
                    status: cell.status.clone(),
                    error: cell.error.clone(),
                    cached: cell.cached,
                })
        })
        .collect();

    let prompt = u64::try_from(outcome.prompt_tokens).unwrap_or(0);
    let completion = u64::try_from(outcome.completion_tokens).unwrap_or(0);
    Ok(Json(SampleRunResponse {
        rows,
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: prompt + completion,
        cached_tokens: u64::try_from(outcome.cached_tokens).unwrap_or(0),
        cache_hits: outcome.cache_hits,
    }))
}

#[derive(Debug, Deserialize)]
pub struct EstimateQuery {
    pub scope: Option<String>,
}

/// `GET /api/functions/{id}/estimate?scope=missing|all|failed`
pub async fn estimate(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<EstimateQuery>,
) -> AppResult<Json<EstimateResponse>> {
    let store = store(&state)?;
    let (row, table_row) = function_context(store, &id).await?;
    let spec = run_spec_from_row(store, &row).await?;
    let scope = q.scope.as_deref().unwrap_or("missing");

    let shash = spec.spec_hash();
    let df = store
        .read_table(&table_row.source_id, &table_row.name)
        .await?;
    let inputs = runner::prepare_run_inputs(&df, &spec)?;
    let mut hashes: Vec<String> = inputs.iter().map(|r| r.hash.clone()).collect();
    hashes.sort_unstable();
    hashes.dedup();
    let cached = store
        .db()
        .get_cached_cells(&row.id, &shash, &hashes)
        .await?;
    let cached_set: std::collections::HashSet<&str> =
        cached.iter().map(|c| c.input_hash.as_str()).collect();
    let error_set: std::collections::HashSet<&str> = cached
        .iter()
        .filter(|c| c.status == "error")
        .map(|c| c.input_hash.as_str())
        .collect();

    let rows_total = i64::try_from(inputs.len()).unwrap_or(i64::MAX);
    let rows_uncached = i64::try_from(
        inputs
            .iter()
            .filter(|r| !cached_set.contains(r.hash.as_str()))
            .count(),
    )
    .unwrap_or(0);
    let rows_failed = i64::try_from(
        inputs
            .iter()
            .filter(|r| error_set.contains(r.hash.as_str()))
            .count(),
    )
    .unwrap_or(0);

    let rows_to_run = match scope {
        "all" => rows_total,
        "failed" => rows_failed,
        _ => rows_uncached,
    };

    let p75 = store.db().cache_stats_p75_tokens(&row.id).await?;
    let (per_row, basis) = match p75 {
        Some(t) if t > 0 => (t, "history"),
        _ => {
            let prompt_chars = i64::try_from(spec.prompt_chars()).unwrap_or(0);
            let outputs = i64::try_from(spec.output_count()).unwrap_or(1);
            (
                prompt_chars / 4 + HEURISTIC_COMPLETION_TOKENS_PER_OUTPUT * outputs.max(1),
                "heuristic",
            )
        },
    };

    Ok(Json(EstimateResponse {
        rows_total,
        rows_uncached,
        rows_to_run,
        p75_tokens_per_row: p75,
        estimated_tokens: rows_to_run.saturating_mul(per_row),
        basis: basis.to_string(),
    }))
}

/// `POST /api/functions/{id}/runs` — start an async full run.
pub async fn start_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<StartEnrichRunRequest>,
) -> AppResult<Json<StartEnrichRunResponse>> {
    if !matches!(req.scope.as_str(), "missing" | "all" | "failed") {
        return Err(AppError::BadRequest(format!(
            "unknown scope '{}' (missing | all | failed)",
            req.scope
        )));
    }
    let store = store(&state)?;
    let (row, _) = function_context(store, &id).await?;
    let run_id = start_run_internal_scoped(&state, &row, &req.scope).await?;
    Ok(Json(StartEnrichRunResponse { run_id }))
}

/// Start a run with scope 'all' or 'missing' (used by PUT rerun + the
/// post-sync hook).
pub(crate) async fn start_run_internal(
    state: &AppState,
    row: &EnrichmentFunctionRow,
    all: bool,
) -> AppResult<String> {
    start_run_internal_scoped(state, row, if all { "all" } else { "missing" }).await
}

async fn start_run_internal_scoped(
    state: &AppState,
    row: &EnrichmentFunctionRow,
    scope: &str,
) -> AppResult<String> {
    let store = state.require_store()?;
    if !super::RUNNABLE_KINDS.contains(&row.kind.as_str()) {
        return Err(AppError::BadRequest(
            "runs are only available for ticket functions".to_string(),
        ));
    }
    if let Some(active) = store.db().active_enrichment_run(&row.id).await? {
        return Err(AppError::Conflict(format!(
            "run {} is already active for this function",
            active.id
        )));
    }
    let table_row = store
        .db()
        .get_table_by_id(&row.table_id)
        .await?
        .ok_or_else(|| AppError::NotFound("owning table not found".to_string()))?;
    let spec = run_spec_from_row(store, row).await?;
    // Fail fast when no provider is configured.
    let (provider_id, model) = spec.provider();
    crate::llm::client_for(state, provider_id, model).await?;

    let shash = spec.spec_hash();
    match scope {
        "all" => {
            store.db().delete_cache_for_spec(&row.id, &shash).await?;
        },
        "failed" => {
            store
                .db()
                .delete_error_cache_for_spec(&row.id, &shash)
                .await?;
        },
        _ => {},
    }

    let mode = if scope == "missing" {
        "incremental"
    } else {
        "full"
    };
    let run = store
        .db()
        .insert_enrichment_run(&row.id, row.current_version, mode, table_row.total_rows)
        .await?;

    let run_id = run.id.clone();
    let task_state = state.clone();
    let handle = tokio::spawn(runner::execute_full_run(
        task_state,
        run.id,
        row.id.clone(),
        row.name.clone(),
        row.current_version,
        table_row.source_id,
        table_row.name,
        spec,
    ));
    state
        .enrichment_jobs
        .insert(run_id.clone(), handle.abort_handle());
    crate::jobs::emit_enrichment_job(state, &run_id).await;
    Ok(run_id)
}

/// `POST /api/functions/{id}/materialize` — rewrite the outputs from the
/// cache without any LLM call. This is how a vocabulary rename or a mapped
/// unresolved subject reaches the table.
pub async fn materialize_only(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let store = store(&state)?;
    let (row, table_row) = function_context(store, &id).await?;
    if !super::RUNNABLE_KINDS.contains(&row.kind.as_str()) {
        return Err(AppError::BadRequest(
            "materialize is only available for per-row functions".to_string(),
        ));
    }
    if let Some(active) = store.db().active_enrichment_run(&row.id).await? {
        return Err(AppError::Conflict(format!(
            "run {} is active for this function — wait for it",
            active.id
        )));
    }
    let spec = run_spec_from_row(store, &row).await?;
    runner::materialize(
        &state,
        store,
        &table_row.source_id,
        &table_row.name,
        &row.id,
        &row.name,
        &spec,
    )
    .await?;
    Ok(Json(serde_json::json!({ "materialized": true })))
}

/// `GET /api/enrichment/runs/{rid}`
pub async fn get_run(
    State(state): State<AppState>,
    Path(rid): Path<String>,
) -> AppResult<Json<EnrichRunResponse>> {
    let store = store(&state)?;
    let run = store
        .db()
        .get_enrichment_run(&rid)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("run {rid} not found")))?;
    Ok(Json(run.into()))
}

/// `POST /api/enrichment/runs/{rid}/cancel`
pub async fn cancel_run(
    State(state): State<AppState>,
    Path(rid): Path<String>,
) -> AppResult<Json<EnrichRunResponse>> {
    let store = store(&state)?;
    let run = store
        .db()
        .get_enrichment_run(&rid)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("run {rid} not found")))?;
    if run.status == "running" {
        if let Some((_, handle)) = state.enrichment_jobs.remove(&rid) {
            handle.abort();
        }
        store
            .db()
            .finish_enrichment_run(&rid, "cancelled", Some("cancelled by user"))
            .await?;
        crate::jobs::emit_enrichment_job(&state, &rid).await;
    }
    let updated = store
        .db()
        .get_enrichment_run(&rid)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("run {rid} not found")))?;
    Ok(Json(updated.into()))
}

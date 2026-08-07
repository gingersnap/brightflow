//! Enrichment-function endpoints: CRUD + versions + lifecycle, synchronous
//! sample runs, token estimates, and async full/incremental runs.
//!
//! This file is HTTP wiring over two siblings: the pure spec rules live in
//! `validate` (unit-tested there), the async run loop in `runner`.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use brightflow_engine::enrichment::{spec_hash, FunctionSpec, LlmPromptSpec};
use brightflow_store::{EnrichmentFunctionRow, ParquetStore, TableRow};

use crate::enrichment::runner;
use crate::enrichment::types::{
    CreateFunctionRequest, EnrichFunctionResponse, EnrichRunResponse, EstimateResponse,
    FunctionVersionResponse, SampleCellResponse, SampleRunRequest, SampleRunResponse,
    StartEnrichRunRequest, StartEnrichRunResponse, UpdateFunctionRequest,
};
use crate::enrichment::validate::{
    parse_spec, table_columns as schema_columns, valid_name, validate_spec,
};
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

/// Column names for a table row's stored schema (see `validate::table_columns`).
fn table_columns(table: &TableRow) -> Vec<String> {
    schema_columns(table.schema_json.as_deref())
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
    let stale_row_count = if row.kind == "llm_prompt" {
        let spec: Option<FunctionSpec> = serde_json::from_str(&version.config_json).ok();
        if let Some(FunctionSpec::LlmPrompt(llm)) = spec {
            let (cached, _errors) = store.db().count_cached(&row.id, &spec_hash(&llm)).await?;
            let total = store
                .db()
                .get_table_by_id(&row.table_id)
                .await?
                .map_or(0, |t| t.total_rows);
            Some((total - cached).max(0))
        } else {
            None
        }
    } else {
        None
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

/// Load the current (or a specific draft) LlmPromptSpec for a function.
fn llm_spec_from_config(config_json: &str) -> AppResult<LlmPromptSpec> {
    match serde_json::from_str::<FunctionSpec>(config_json) {
        Ok(FunctionSpec::LlmPrompt(spec)) => Ok(spec),
        Ok(_) => Err(AppError::BadRequest(
            "this operation is only available for llm_prompt functions".to_string(),
        )),
        Err(e) => Err(AppError::Internal(format!("stored config unreadable: {e}"))),
    }
}

/// `POST /api/sources/{source_id}/tables/{table}/functions`
pub async fn create_function(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
    Json(req): Json<CreateFunctionRequest>,
) -> AppResult<Json<EnrichFunctionResponse>> {
    let store = store(&state)?;
    if !matches!(
        req.kind.as_str(),
        "llm_prompt" | "topic_model" | "classifier"
    ) {
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
    let columns = table_columns(&table_row);
    if columns.iter().any(|c| c == &req.name) {
        return Err(AppError::BadRequest(format!(
            "function name '{}' collides with an existing table column",
            req.name
        )));
    }

    let spec = parse_spec(&req.kind, &req.config)?;
    validate_spec(&spec, &columns)?;
    let config_json = serde_json::to_string(&spec).map_err(AppError::Json)?;

    let created = store
        .db()
        .create_enrichment_function(&table_row.id, &req.name, &req.kind, "draft", &config_json)
        .await
        .map_err(|e| {
            if e.is_unique_violation() {
                AppError::Conflict(format!(
                    "a function named '{}' (or a topic model) already exists on this table",
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

    let spec = parse_spec(&row.kind, &req.config)?;
    let columns = table_columns(&table_row);
    // The function's own materialized outputs may already be table columns;
    // exclude them from the collision check on edit.
    let previous = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?;
    let own_outputs: Vec<String> = previous
        .as_ref()
        .and_then(|v| serde_json::from_str::<FunctionSpec>(&v.config_json).ok())
        .map(|old| match old {
            FunctionSpec::LlmPrompt(llm) => llm.outputs.iter().map(|f| f.name.clone()).collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default();
    let mut status_col = own_outputs.clone();
    status_col.push(format!("{}__status", row.name));
    let filtered: Vec<String> = columns
        .into_iter()
        .filter(|c| !status_col.contains(c))
        .collect();
    validate_spec(&spec, &filtered)?;

    let config_json = serde_json::to_string(&spec).map_err(AppError::Json)?;
    store
        .db()
        .update_enrichment_function_config(&row.id, &config_json)
        .await?;

    // Cache housekeeping: keep only the new and previous spec's cells.
    if let FunctionSpec::LlmPrompt(new_llm) = &spec {
        let mut keep = vec![spec_hash(new_llm)];
        if let Some(FunctionSpec::LlmPrompt(prev_llm)) = previous
            .as_ref()
            .and_then(|v| serde_json::from_str::<FunctionSpec>(&v.config_json).ok())
        {
            keep.push(spec_hash(&prev_llm));
        }
        keep.dedup();
        store.db().prune_cache_except(&row.id, &keep).await?;
    }

    let updated = store
        .db()
        .get_enrichment_function(&row.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("function {id} not found")))?;

    if rerun != "none" {
        if let FunctionSpec::LlmPrompt(_) = &spec {
            // Best effort: a conflicting active run leaves the edit saved.
            if let Err(e) = start_run_internal(&state, &updated, rerun == "all").await {
                tracing::warn!("post-edit rerun not started: {e}");
            }
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

    if q.drop_columns.unwrap_or(false) && row.kind == "llm_prompt" {
        let version = store
            .db()
            .get_enrichment_function_version(&row.id, row.current_version)
            .await?;
        if let Some(v) = version {
            if let Ok(spec) = llm_spec_from_config(&v.config_json) {
                let mut names: Vec<String> = spec.outputs.iter().map(|f| f.name.clone()).collect();
                names.push(format!("{}__status", row.name));
                let df = store
                    .read_table(&table_row.source_id, &table_row.name)
                    .await?;
                let mut out_df = df;
                let mut dropped_any = false;
                for name in &names {
                    if out_df.column(name).is_ok() {
                        out_df = out_df.drop(name).map_err(AppError::Polars)?;
                        dropped_any = true;
                    }
                }
                if dropped_any {
                    store
                        .replace_table_data(&table_row.source_id, &table_row.name, out_df, None)
                        .await?;
                    state.invalidate_schema_cache(&crate::state::cache_key(
                        &table_row.source_id,
                        &table_row.name,
                    ));
                }
            }
        }
    }

    store.db().delete_enrichment_function(&row.id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
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

/// `POST /api/functions/{id}/promote`
pub async fn promote_function(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<EnrichFunctionResponse>> {
    set_status(&state, &id, "promoted").await
}

/// `POST /api/functions/{id}/demote`
pub async fn demote_function(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<EnrichFunctionResponse>> {
    set_status(&state, &id, "draft").await
}

async fn set_status(
    state: &AppState,
    id: &str,
    status: &str,
) -> AppResult<Json<EnrichFunctionResponse>> {
    let store = store(state)?;
    let (row, _) = function_context(store, id).await?;
    store
        .db()
        .set_enrichment_function_status(&row.id, status)
        .await?;
    let updated = store
        .db()
        .get_enrichment_function(&row.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("function {id} not found")))?;
    Ok(Json(to_response(store, &updated).await?))
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
    if row.kind != "llm_prompt" {
        return Err(AppError::BadRequest(
            "sample runs are only available for llm_prompt functions".to_string(),
        ));
    }

    let spec = if let Some(draft) = &req.config {
        let parsed = parse_spec(&row.kind, draft)?;
        let columns = table_columns(&table_row);
        let own: Vec<String> = vec![format!("{}__status", row.name)];
        let filtered: Vec<String> = columns.into_iter().filter(|c| !own.contains(c)).collect();
        validate_spec(&parsed, &filtered)?;
        match parsed {
            FunctionSpec::LlmPrompt(llm) => llm,
            FunctionSpec::TopicModel(_) | FunctionSpec::Classifier(_) => {
                return Err(AppError::BadRequest(
                    "draft config must be an llm_prompt spec".to_string(),
                ))
            },
        }
    } else {
        let version = store
            .db()
            .get_enrichment_function_version(&row.id, row.current_version)
            .await?
            .ok_or_else(|| AppError::Internal("missing version snapshot".to_string()))?;
        llm_spec_from_config(&version.config_json)?
    };

    let client = crate::llm::client_for(&state, &spec.provider_id, spec.model.as_deref()).await?;
    let limit = req
        .limit
        .unwrap_or(DEFAULT_SAMPLE_ROWS)
        .clamp(1, MAX_SAMPLE_ROWS);
    let df = store
        .read_table(&table_row.source_id, &table_row.name)
        .await?;
    let sample = df.head(Some(limit));
    let inputs = runner::prepare_inputs(&sample, &spec)?;

    let outcome = runner::execute_cells(
        store,
        &client,
        &row.id,
        row.current_version,
        &spec,
        &inputs,
        None,
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
                    value: cell.value.clone(),
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
    let version = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?
        .ok_or_else(|| AppError::Internal("missing version snapshot".to_string()))?;
    let spec = llm_spec_from_config(&version.config_json)?;
    let scope = q.scope.as_deref().unwrap_or("missing");

    let shash = spec_hash(&spec);
    let df = store
        .read_table(&table_row.source_id, &table_row.name)
        .await?;
    let inputs = runner::prepare_inputs(&df, &spec)?;
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
            let prompt_chars = i64::try_from(spec.prompt_template.len()).unwrap_or(0);
            let outputs = i64::try_from(spec.outputs.len()).unwrap_or(1);
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
    if row.kind != "llm_prompt" {
        return Err(AppError::BadRequest(
            "runs are only available for llm_prompt functions (topics run via recluster)"
                .to_string(),
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
    let version = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?
        .ok_or_else(|| AppError::Internal("missing version snapshot".to_string()))?;
    let spec = llm_spec_from_config(&version.config_json)?;
    // Fail fast when no provider is configured.
    crate::llm::client_for(state, &spec.provider_id, spec.model.as_deref()).await?;

    let shash = spec_hash(&spec);
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
    Ok(run_id)
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
    }
    let updated = store
        .db()
        .get_enrichment_run(&rid)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("run {rid} not found")))?;
    Ok(Json(updated.into()))
}

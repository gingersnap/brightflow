//! Agent run lifecycle endpoints.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::agent::runner;
use crate::agent::types::{AgentRunResponse, StartAgentRunRequest};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Must stay in sync with `runner::tools_for`, `runner::build_context`, and the
/// `agent_runs.kind` CHECK constraint (migration 023).
const VALID_KINDS: &[&str] = &[
    "narrate_insights",
    "triage_insights",
    "propose_categories",
    "propose_subcategories",
    "propose_feedback_categories",
];

/// Must stay inside the `agent_runs.mode` CHECK constraint (migration 010).
const VALID_MODES: &[&str] = &["propose", "auto_apply"];

pub(crate) fn to_response(
    row: brightflow_store::AgentRunRow,
    actions: Vec<i64>,
) -> AgentRunResponse {
    AgentRunResponse {
        id: row.id,
        kind: row.kind,
        mode: row.mode,
        scope: row.scope,
        status: row.status,
        detail: row.detail,
        created_at: row.created_at,
        finished_at: row.finished_at,
        proposed_actions: actions,
    }
}

/// `POST /api/agent/runs` — start a run. 409 when one is active for the scope.
pub async fn start_run(
    State(state): State<AppState>,
    Json(req): Json<StartAgentRunRequest>,
) -> AppResult<Json<AgentRunResponse>> {
    if !VALID_KINDS.contains(&req.kind.as_str()) {
        return Err(AppError::BadRequest(format!(
            "unknown agent kind '{}'; valid: {}",
            req.kind,
            VALID_KINDS.join(", ")
        )));
    }
    let mode = req.mode.as_deref().unwrap_or("auto_apply");
    if !VALID_MODES.contains(&mode) {
        return Err(AppError::BadRequest(format!(
            "unknown agent mode '{mode}'; valid: {}",
            VALID_MODES.join(", ")
        )));
    }
    // Fail fast when no provider is configured.
    crate::llm::default_client(&state).await?;

    let store = state.require_store()?;
    if req.kind == "propose_subcategories" && req.parent_id.is_none() {
        return Err(AppError::BadRequest(
            "propose_subcategories needs parent_id — subcategories are induced per parent"
                .to_string(),
        ));
    }
    let scope = match req.parent_id {
        Some(parent) if req.kind == "propose_subcategories" => {
            format!("{}:{}:{}:{parent}", req.kind, req.source_id, req.table)
        },
        _ => format!("{}:{}:{}", req.kind, req.source_id, req.table),
    };
    if let Some(active) = store.db().active_agent_run_for_scope(&scope).await? {
        return Err(AppError::Conflict(format!(
            "agent run {} is already running for this scope",
            active.id
        )));
    }
    let row = store
        .db()
        .insert_agent_run(&req.kind, mode, &scope, chrono::Utc::now().timestamp())
        .await?;
    let run_id = row.id;
    let auto_apply = mode == "auto_apply";

    let task_state = state.clone();
    let kind = req.kind.clone();
    let source_id = req.source_id.clone();
    let table = req.table.clone();
    let parent_id = req.parent_id;
    let handle = tokio::spawn(async move {
        runner::execute_run(
            task_state, run_id, kind, source_id, table, auto_apply, parent_id,
        )
        .await;
    });
    state.agent_runs.insert(run_id, handle.abort_handle());

    crate::actions::events::emit_run_row(&state, row.clone(), Vec::new());
    Ok(Json(to_response(row, Vec::new())))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
}

/// `GET /api/agent/runs`
pub async fn list_runs(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<AgentRunResponse>>> {
    let store = state.require_store()?;
    let rows = store
        .db()
        .list_agent_runs(q.limit.unwrap_or(50).clamp(1, 200))
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| to_response(r, Vec::new()))
            .collect(),
    ))
}

/// `GET /api/agent/runs/{id}` — run + the actions it proposed.
pub async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<AgentRunResponse>> {
    let store = state.require_store()?;
    let row = store
        .db()
        .get_agent_run(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("agent run {id} not found")))?;
    let actions = store
        .db()
        .list_actions_for_agent_run(id)
        .await?
        .into_iter()
        .map(|a| a.id)
        .collect();
    Ok(Json(to_response(row, actions)))
}

/// `POST /api/agent/runs/{id}/undo-all` — run-level bulk undo.
///
/// Reverts every applied, undoable action of a run, **newest first**:
/// dependent inverses unwind in reverse application order (a category rename
/// must be undone before the define that created it).
///
/// Continues on per-row failure and reports what could not be reverted —
/// inverses are blind to interleaved edits (except the taxonomy-label
/// guard), so a partial bulk undo leaves the run half-reverted by design.
pub async fn undo_all(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<crate::actions::types::BulkUndoResponse>> {
    use crate::actions::handlers::{push_failure, undo_action_row};
    use crate::actions::types::ActionLogEntry;

    let store = state.require_store()?;
    store
        .db()
        .get_agent_run(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("agent run {id} not found")))?;

    let rows: Vec<brightflow_store::ActionLogRow> = store
        .db()
        .list_actions_for_agent_run(id)
        .await?
        .into_iter()
        .filter(|r| r.status == "applied" && r.undo_json.is_some())
        .collect();

    let total = rows.len();
    let mut undone = 0usize;
    let mut failures = Vec::new();
    let mut entries = Vec::with_capacity(total);

    for mut row in rows.into_iter().rev() {
        match undo_action_row(&state, &row).await {
            Ok(()) => {
                undone += 1;
                row.status = "undone".to_string();
                row.resolved_at = Some(chrono::Utc::now().timestamp());
                entries.push(ActionLogEntry::from_row(row));
            },
            Err(e) => push_failure(&mut failures, row.id, &row.action_kind, e.to_string()),
        }
    }

    let failed = total.saturating_sub(undone);
    tracing::info!("bulk undo (run {id}): {undone}/{total} undone, {failed} failed");
    crate::actions::events::emit_batch(&state, entries, total, undone, failed).await;
    Ok(Json(crate::actions::types::BulkUndoResponse {
        total,
        undone,
        failed,
        failures,
    }))
}

/// `POST /api/agent/runs/{id}/cancel`
pub async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<AgentRunResponse>> {
    let store = state.require_store()?;
    let row = store
        .db()
        .get_agent_run(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("agent run {id} not found")))?;
    if row.status == "running" {
        if let Some((_, handle)) = state.agent_runs.remove(&id) {
            handle.abort();
        }
        store
            .db()
            .finish_agent_run(
                id,
                "cancelled",
                Some("cancelled by user"),
                chrono::Utc::now().timestamp(),
            )
            .await?;
    }
    let updated = store
        .db()
        .get_agent_run(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("agent run {id} not found")))?;
    crate::actions::events::emit_run_row(&state, updated.clone(), Vec::new());
    Ok(Json(to_response(updated, Vec::new())))
}

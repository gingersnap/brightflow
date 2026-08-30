//! Action dispatch: the single execution path for every curation operation.
//!
//! `POST /api/actions` is idempotent on `request_id`. Human actions apply
//! immediately (undo available). Agent actions are reversibility-tiered:
//! in auto-apply mode, undoable kinds apply immediately exactly like human
//! actions; anything else queues as proposed — the SAME `execute_action`
//! runs on approval.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::actions::events;
use crate::actions::types::{
    Action, ActionLogEntry, ActionManifestEntry, ActionRequest, ActionResponse, ActionStatus,
    BulkApproveFailure, BulkApproveResponse, PendingCount, Scope, UndoOp, ACTION_KINDS,
};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Who performed an action. The agent runner (3C) passes `Agent`; the REST
/// endpoint passes the session's user so the audit line can name them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Human { user_id: String },
    Agent { run_id: i64, auto_apply: bool },
}

impl Actor {
    fn type_str(&self) -> &'static str {
        match self {
            Self::Human { .. } => "human",
            Self::Agent { .. } => "agent",
        }
    }

    fn agent_run_id(&self) -> Option<i64> {
        match self {
            Self::Human { .. } => None,
            Self::Agent { run_id, .. } => Some(*run_id),
        }
    }

    fn user_id(&self) -> Option<&str> {
        match self {
            Self::Human { user_id } => Some(user_id),
            Self::Agent { .. } => None,
        }
    }
}

/// Reversibility-tiered initial status.
///
/// Humans apply immediately (undo is the safety net). Agents in auto-apply
/// mode get the same deal — but only for kinds whose undo genuinely exists;
/// anything irreversible still queues for approval regardless of mode.
fn initial_status(actor: &Actor, kind_undoable: bool) -> &'static str {
    match actor {
        Actor::Human { .. } => "applied",
        Actor::Agent {
            auto_apply: true, ..
        } if kind_undoable => "applied",
        Actor::Agent { .. } => "proposed",
    }
}

/// `POST /api/actions` — dispatch an action as a human actor.
pub async fn dispatch(
    State(state): State<AppState>,
    auth_session: crate::auth::AuthSession,
    Json(req): Json<ActionRequest>,
) -> AppResult<Json<ActionResponse>> {
    // The route sits behind `require_auth`, so a missing user here is a
    // wiring error, not an anonymous caller — refuse rather than log a blank.
    let user_id = auth_session
        .user
        .as_ref()
        .map(|u| u.id.clone())
        .ok_or(AppError::Unauthorized)?;
    let response = dispatch_action(
        &state,
        req.action,
        &req.request_id,
        Actor::Human { user_id },
    )
    .await?;
    Ok(Json(response))
}

/// Shared dispatch used by the REST endpoint (human) and the agent runner.
/// See `initial_status` for which actions execute here vs queue as proposed.
pub async fn dispatch_action(
    state: &AppState,
    action: Action,
    request_id: &str,
    actor: Actor,
) -> AppResult<ActionResponse> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    let params_json = serde_json::to_string(&action)
        .map_err(|e| AppError::Internal(format!("action serialize: {e}")))?;

    let status = initial_status(
        &actor,
        crate::actions::types::kind_is_undoable(action.kind()),
    );
    let inserted = store
        .db()
        .insert_action(
            request_id,
            actor.type_str(),
            actor.agent_run_id(),
            actor.user_id(),
            action.kind(),
            &params_json,
            status,
            chrono::Utc::now().timestamp(),
        )
        .await?;

    let Some(row) = inserted else {
        // Idempotent replay: return the stored outcome.
        let existing = store
            .db()
            .get_action_by_request_id(request_id)
            .await?
            .ok_or_else(|| AppError::Internal("request_id conflict but row missing".into()))?;
        return Ok(ActionResponse {
            log_id: existing.id,
            status: parse_status(&existing.status),
            result: existing
                .result_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok())
                .unwrap_or(serde_json::Value::Null),
        });
    };

    if status == "proposed" {
        let log_id = row.id;
        events::emit_action(state, ActionLogEntry::from_row(row)).await;
        return Ok(ActionResponse {
            log_id,
            status: ActionStatus::Proposed,
            result: json!({ "proposed": true }),
        });
    }

    // Human, or auto-apply agent with an undoable kind: execute now. The
    // undo op is captured at apply time — identical semantics either way.
    execute_and_record(state, row, action).await
}

/// Execute an action, persist result/undo onto its log row, and emit the
/// updated entry on the curation bus.
async fn execute_and_record(
    state: &AppState,
    row: brightflow_store::ActionLogRow,
    action: Action,
) -> AppResult<ActionResponse> {
    let (response, entry) = execute_and_record_quiet(state, row, action).await?;
    events::emit_action(state, entry).await;
    Ok(response)
}

/// `execute_and_record` without the per-row WS emit — `approve_all` runs this
/// in a loop and emits one batch event instead. Returns the updated feed
/// entry, built from the row in hand (no re-fetch).
async fn execute_and_record_quiet(
    state: &AppState,
    mut row: brightflow_store::ActionLogRow,
    action: Action,
) -> AppResult<(ActionResponse, ActionLogEntry)> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    let log_id = row.id;
    let now = chrono::Utc::now().timestamp();
    match execute_action(state, &action).await {
        Ok((result, undo)) => {
            let undo_json = undo
                .map(|u| serde_json::to_string(&u))
                .transpose()
                .map_err(|e| AppError::Internal(format!("undo serialize: {e}")))?;
            store
                .db()
                .update_action_result(
                    log_id,
                    "applied",
                    Some(&result.to_string()),
                    undo_json.as_deref(),
                    now,
                )
                .await?;
            row.status = "applied".to_string();
            row.result_json = Some(result.to_string());
            row.undo_json = undo_json;
            row.resolved_at = Some(now);
            Ok((
                ActionResponse {
                    log_id,
                    status: ActionStatus::Applied,
                    result,
                },
                ActionLogEntry::from_row(row),
            ))
        },
        Err(e) => {
            let msg = e.to_string();
            let result_json = json!({ "error": msg }).to_string();
            store
                .db()
                .update_action_result(log_id, "failed", Some(&result_json), None, now)
                .await?;
            row.status = "failed".to_string();
            row.result_json = Some(result_json);
            row.undo_json = None;
            row.resolved_at = Some(now);
            Ok((
                ActionResponse {
                    log_id,
                    status: ActionStatus::Failed,
                    result: json!({ "error": msg }),
                },
                ActionLogEntry::from_row(row),
            ))
        },
    }
}

fn parse_status(s: &str) -> ActionStatus {
    match s {
        "proposed" => ActionStatus::Proposed,
        "rejected" => ActionStatus::Rejected,
        "undone" => ActionStatus::Undone,
        "failed" => ActionStatus::Failed,
        _ => ActionStatus::Applied,
    }
}

#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    pub limit: Option<i64>,
}

/// `GET /api/actions` — reverse-chronological audit feed.
pub async fn feed(
    State(state): State<AppState>,
    Query(q): Query<FeedQuery>,
) -> AppResult<Json<Vec<ActionLogEntry>>> {
    let store = state.require_store()?;
    let rows = store
        .db()
        .list_actions(q.limit.unwrap_or(100).clamp(1, 500))
        .await?;
    let entries = rows.into_iter().map(ActionLogEntry::from_row).collect();
    Ok(Json(entries))
}

/// `GET /api/actions/manifest` — action catalog (also the LLM tool registry).
pub async fn manifest() -> Json<Vec<ActionManifestEntry>> {
    let root = schemars::schema_for!(Action);
    let root_value = serde_json::to_value(&root).unwrap_or(serde_json::Value::Null);
    // The internally-tagged enum schema is a oneOf; index variants by their tag.
    let variants: Vec<serde_json::Value> = root_value
        .get("oneOf")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let schema_for_kind = |kind: &str| -> serde_json::Value {
        variants
            .iter()
            .find(|v| {
                v.pointer("/properties/kind/const")
                    .and_then(|c| c.as_str())
                    .is_some_and(|c| c == kind)
            })
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    };
    Json(
        ACTION_KINDS
            .iter()
            .map(|(kind, label, description, undoable)| ActionManifestEntry {
                kind: (*kind).to_string(),
                label: (*label).to_string(),
                description: (*description).to_string(),
                undoable: *undoable,
                schema: schema_for_kind(kind),
            })
            .collect(),
    )
}

/// `POST /api/actions/{id}/approve` — execute a proposed (agent) action.
pub async fn approve(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ActionResponse>> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    let row = store
        .db()
        .get_action(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("action {id} not found")))?;
    if row.status != "proposed" {
        return Err(AppError::BadRequest(format!(
            "action {id} is '{}', only proposed actions can be approved",
            row.status
        )));
    }
    let action: Action = serde_json::from_str(&row.params_json)
        .map_err(|e| AppError::Internal(format!("stored action unreadable: {e}")))?;
    execute_and_record(&state, row, action).await.map(Json)
}

/// Cap on how many failure details a bulk operation reports back.
pub(crate) const MAX_REPORTED_FAILURES: usize = 20;

/// `GET /api/actions/pending-count` — proposals awaiting review.
///
/// Separate from the feed because the feed is truncated (100 by default) while
/// a `label_documents` run can propose a thousand. Counting the visible page
/// would put a wrong number on the "approve all" button.
pub async fn pending_count(State(state): State<AppState>) -> AppResult<Json<PendingCount>> {
    let store = state.require_store()?;
    let count = store.db().count_proposed_actions().await?;
    Ok(Json(PendingCount {
        count: usize::try_from(count).unwrap_or(0),
    }))
}

/// `POST /api/actions/approve-all` — approve every pending proposal.
///
/// Applies them **oldest first** because proposals have ordering dependencies
/// (a category must exist before a label can name it), and runs each through the
/// same `execute_and_record` a single approval uses — so the audit log, undo
/// records and failure handling are identical. Nothing here is a shortcut around
/// the normal path; it is the normal path in a loop.
pub async fn approve_all(State(state): State<AppState>) -> AppResult<Json<BulkApproveResponse>> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    let rows = store.db().list_proposed_actions().await?;

    let total = rows.len();
    let mut approved = 0usize;
    let mut failures: Vec<BulkApproveFailure> = Vec::new();
    let mut entries: Vec<ActionLogEntry> = Vec::with_capacity(total);

    for mut row in rows {
        // A proposal whose stored params no longer deserialize is a failure for
        // that row alone — record it and keep going.
        let parsed: Result<Action, _> = serde_json::from_str(&row.params_json);
        let action = match parsed {
            Ok(a) => a,
            Err(e) => {
                push_failure(
                    &mut failures,
                    row.id,
                    &row.action_kind,
                    format!("stored action unreadable: {e}"),
                );
                // Mark it failed so it stops showing as pending forever.
                let now = chrono::Utc::now().timestamp();
                if let Err(db_err) = store
                    .db()
                    .update_action_result(row.id, "failed", None, None, now)
                    .await
                {
                    tracing::warn!("could not mark action {} failed: {db_err}", row.id);
                } else {
                    row.status = "failed".to_string();
                    row.resolved_at = Some(now);
                    entries.push(ActionLogEntry::from_row(row));
                }
                continue;
            },
        };
        let (log_id, kind) = (row.id, row.action_kind.clone());
        // Quiet per-row execution: one batch event goes out at the end
        // instead of a WS frame per proposal.
        match execute_and_record_quiet(&state, row, action).await {
            Ok((response, entry)) => {
                if response.status == ActionStatus::Applied {
                    approved += 1;
                } else {
                    let detail = response
                        .result
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("action did not apply")
                        .to_string();
                    push_failure(&mut failures, log_id, &kind, detail);
                }
                entries.push(entry);
            },
            Err(e) => push_failure(&mut failures, log_id, &kind, e.to_string()),
        }
    }

    let failed = total.saturating_sub(approved);
    tracing::info!("bulk approve: {approved}/{total} applied, {failed} failed");
    events::emit_batch(&state, entries, total, approved, failed).await;
    Ok(Json(BulkApproveResponse {
        total,
        approved,
        failed,
        failures,
    }))
}

pub(crate) fn push_failure(
    out: &mut Vec<BulkApproveFailure>,
    log_id: i64,
    kind: &str,
    error: String,
) {
    if out.len() < MAX_REPORTED_FAILURES {
        out.push(BulkApproveFailure {
            log_id,
            action_kind: kind.to_string(),
            error,
        });
    }
}

/// `POST /api/actions/{id}/reject`
pub async fn reject(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ActionResponse>> {
    let store = state.require_store()?;
    let row = store
        .db()
        .get_action(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("action {id} not found")))?;
    if row.status != "proposed" {
        return Err(AppError::BadRequest(format!(
            "action {id} is '{}', only proposed actions can be rejected",
            row.status
        )));
    }
    store
        .db()
        .set_action_status(id, "rejected", chrono::Utc::now().timestamp())
        .await?;
    // Rare path: a re-fetch keeps the emit simple.
    if let Ok(Some(updated)) = store.db().get_action(id).await {
        events::emit_action(&state, ActionLogEntry::from_row(updated)).await;
    }
    Ok(Json(ActionResponse {
        log_id: id,
        status: ActionStatus::Rejected,
        result: serde_json::Value::Null,
    }))
}

/// Undo one applied action row: apply the stored inverse and flip the status
/// to `undone`. Shared by the single-undo endpoint and run-level bulk undo.
/// Does NOT emit — callers decide between per-action and batch events.
pub(crate) async fn undo_action_row(
    state: &AppState,
    row: &brightflow_store::ActionLogRow,
) -> AppResult<()> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    if row.status != "applied" {
        return Err(AppError::BadRequest(format!(
            "action {} is '{}', only applied actions can be undone",
            row.id, row.status
        )));
    }
    let Some(undo_json) = row.undo_json.as_deref() else {
        return Err(AppError::BadRequest(format!(
            "action '{}' is not undoable",
            row.action_kind
        )));
    };
    let op: UndoOp = serde_json::from_str(undo_json)
        .map_err(|e| AppError::Internal(format!("stored undo unreadable: {e}")))?;
    apply_undo(state, &op).await?;
    store
        .db()
        .set_action_status(row.id, "undone", chrono::Utc::now().timestamp())
        .await?;
    Ok(())
}

/// `POST /api/actions/{id}/undo`
pub async fn undo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ActionResponse>> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    let row = store
        .db()
        .get_action(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("action {id} not found")))?;
    undo_action_row(&state, &row).await?;
    // Rare path: a re-fetch keeps the emit simple.
    if let Ok(Some(updated)) = store.db().get_action(id).await {
        events::emit_action(&state, ActionLogEntry::from_row(updated)).await;
    }
    Ok(Json(ActionResponse {
        log_id: id,
        status: ActionStatus::Undone,
        result: json!({ "undone": true }),
    }))
}

// ─── Execution ────────────────────────────────────────────────────────────────

/// Execute one action. Returns (result payload, inverse op when undoable).
///
/// One line per arm, no wildcard: the bodies live in `exec::*`, adjacent to
/// their undos, and a new `Action` variant fails to compile until it has one.
pub async fn execute_action(
    state: &AppState,
    action: &Action,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    use crate::actions::exec::{clusters, insights, semantics, taxonomy};
    match action {
        Action::RenameCluster {
            scope: Scope { source_id, table },
            cluster_id,
            name,
        } => clusters::execute_rename_cluster(state, source_id, table, *cluster_id, name).await,
        Action::MergeClusters {
            scope: Scope { source_id, table },
            from_cluster_id,
            into_cluster_id,
        } => {
            clusters::execute_merge_clusters(
                state,
                source_id,
                table,
                *from_cluster_id,
                *into_cluster_id,
            )
            .await
        },
        Action::MarkClusterNoise {
            scope: Scope { source_id, table },
            cluster_id,
            is_noise,
        } => {
            clusters::execute_mark_cluster_noise(state, source_id, table, *cluster_id, *is_noise)
                .await
        },
        Action::AssignClusterLabel {
            scope: Scope { source_id, table },
            cluster_id,
            label,
        } => {
            clusters::execute_assign_cluster_label(state, source_id, table, *cluster_id, label)
                .await
        },
        Action::ExcludeTerm {
            scope: Scope { source_id, table },
            term,
        } => clusters::execute_exclude_term(state, source_id, table, term).await,
        Action::SplitCluster {
            scope: Scope { source_id, table },
            ..
        } => clusters::execute_split_cluster(state, source_id, table).await,
        Action::Recluster {
            scope: Scope { source_id, table },
            k,
            language,
            embedder,
            min_cluster_size,
            algorithm,
        } => {
            clusters::execute_recluster(
                state,
                source_id,
                table,
                &clusters::ReclusterArgs {
                    k: *k,
                    language: language.as_deref(),
                    embedder: embedder.as_deref(),
                    min_cluster_size: *min_cluster_size,
                    algorithm: algorithm.as_deref(),
                },
            )
            .await
        },
        Action::DismissInsight {
            scope: Scope { source_id, table },
            fingerprint,
            reason,
        } => insights::execute_dismiss_insight(state, source_id, table, fingerprint, reason).await,
        Action::PinInsight {
            scope: Scope { source_id, table },
            fingerprint,
            pinned,
        } => insights::execute_pin_insight(state, source_id, table, fingerprint, *pinned).await,
        Action::AnnotateInsight {
            scope: Scope { source_id, table },
            fingerprint,
            note,
        } => insights::execute_annotate_insight(state, source_id, table, fingerprint, note).await,
        Action::SuppressTarget {
            scope: Scope { source_id, table },
            target_kind,
            target,
        } => insights::execute_suppress_target(state, source_id, table, target_kind, target).await,
        Action::SetKpi {
            scope: Scope { source_id, table },
            column,
            is_kpi,
        } => semantics::execute_set_kpi(state, source_id, table, column, *is_kpi).await,
        Action::SetColumnPolarity {
            scope: Scope { source_id, table },
            column,
            polarity,
        } => {
            semantics::execute_set_column_polarity(state, source_id, table, column, polarity).await
        },
        Action::DefineTaxonomyCategory {
            scope: Scope { source_id, table },
            name,
            description,
            vocab_kind,
            parent_id,
            aliases,
        } => {
            taxonomy::execute_define_taxonomy_category(
                state,
                source_id,
                table,
                &taxonomy::DefineArgs {
                    name,
                    description: description.as_deref(),
                    kind: vocab_kind.as_deref(),
                    parent_id: parent_id.unwrap_or(0),
                    aliases: aliases.as_deref().unwrap_or(&[]),
                },
            )
            .await
        },
        Action::RenameTaxonomyCategory {
            scope: Scope { source_id, table },
            category_id,
            name,
        } => {
            taxonomy::execute_rename_taxonomy_category(state, source_id, table, *category_id, name)
                .await
        },
        Action::RedefineTaxonomyCategory {
            scope: Scope { source_id, table },
            category_id,
            description,
        } => {
            taxonomy::execute_redefine_taxonomy_category(
                state,
                source_id,
                table,
                *category_id,
                description.as_deref(),
            )
            .await
        },
        Action::FreezeTaxonomyCategory {
            scope: Scope { source_id, table },
            category_id,
            frozen,
        } => {
            taxonomy::execute_freeze_taxonomy_category(
                state,
                source_id,
                table,
                *category_id,
                *frozen,
            )
            .await
        },
        Action::DeleteTaxonomyCategory {
            scope: Scope { source_id, table },
            category_id,
        } => {
            taxonomy::execute_delete_taxonomy_category(state, source_id, table, *category_id).await
        },
        Action::LabelDocument {
            scope: Scope { source_id, table },
            row_id,
            categories,
        } => taxonomy::execute_label_document(state, source_id, table, row_id, categories).await,
    }
}

/// Apply an inverse operation. Same shape as `execute_action`: one line per
/// arm, bodies in `exec::*` next to the executes they invert.
pub async fn apply_undo(state: &AppState, op: &UndoOp) -> AppResult<()> {
    use crate::actions::exec::{clusters, insights, semantics, taxonomy};
    match op {
        UndoOp::RestoreClusterEdit {
            table_id,
            centroid_fingerprint,
            centroid_json,
            cluster_id,
            custom_name,
            label,
            is_noise,
            merged_into,
            delete_row,
            refresh_labels,
        } => {
            clusters::undo_restore_cluster_edit(
                state,
                &clusters::RestoreClusterEditArgs {
                    table_id,
                    centroid_fingerprint,
                    centroid_json,
                    cluster_id: *cluster_id,
                    custom_name,
                    label,
                    is_noise: *is_noise,
                    merged_into: *merged_into,
                    delete_row: *delete_row,
                    refresh_labels: *refresh_labels,
                },
            )
            .await
        },
        UndoOp::RemoveExcludedTerm { table_id, term } => {
            clusters::undo_remove_excluded_term(state, table_id, term).await
        },
        UndoOp::DeleteInsightState {
            table_id,
            fingerprint,
        } => insights::undo_delete_insight_state(state, table_id, fingerprint).await,
        UndoOp::RestoreInsightState {
            table_id,
            fingerprint,
            state: insight_state,
            reason,
            annotation,
        } => {
            insights::undo_restore_insight_state(
                state,
                table_id,
                fingerprint,
                insight_state,
                reason.as_deref(),
                annotation.as_deref(),
            )
            .await
        },
        UndoOp::DeleteSuppression {
            table_id,
            kind,
            target,
        } => insights::undo_delete_suppression(state, table_id, kind, target).await,
        UndoOp::RestoreKpi {
            source_id,
            table,
            column,
            role,
            is_kpi,
        } => semantics::undo_restore_kpi(state, source_id, table, column, role, *is_kpi).await,
        UndoOp::RestorePolarity {
            source_id,
            table,
            column,
            polarity,
        } => semantics::undo_restore_polarity(state, source_id, table, column, polarity).await,
        UndoOp::RestoreTaxonomyCategory {
            category_id,
            delete_row,
            name,
            description,
        } => {
            taxonomy::undo_restore_taxonomy_category(
                state,
                *category_id,
                *delete_row,
                name.as_deref(),
                description.as_deref(),
            )
            .await
        },
        UndoOp::RestoreTaxonomyFrozen {
            category_id,
            frozen,
        } => taxonomy::undo_restore_taxonomy_frozen(state, *category_id, *frozen).await,
        UndoOp::RecreateTaxonomyCategory {
            table_id,
            category_id,
            name,
            description,
            created_at,
            labels,
            kind,
            parent_id,
            frozen,
            aliases_json,
        } => {
            let row = brightflow_store::TaxonomyCategoryRow {
                id: *category_id,
                table_id: table_id.clone(),
                kind: kind.clone(),
                parent_id: *parent_id,
                name: name.clone(),
                description: description.clone(),
                frozen: *frozen,
                aliases_json: aliases_json.clone(),
                created_at: *created_at,
            };
            taxonomy::undo_recreate_taxonomy_category(state, &row, labels).await
        },
        UndoOp::RestoreDocumentLabels {
            table_id,
            row_id,
            labels,
        } => taxonomy::undo_restore_document_labels(state, table_id, row_id, labels).await,
    }
}

#[cfg(test)]
mod tests {
    use super::{initial_status, Actor};

    /// The reversibility tier, pinned over all four combinations.
    #[test]
    fn initial_status_tiers_by_actor_and_undoability() {
        let human = Actor::Human {
            user_id: "u1".to_string(),
        };
        assert_eq!(initial_status(&human, true), "applied");
        assert_eq!(initial_status(&human, false), "applied");
        assert_eq!(
            initial_status(
                &Actor::Agent {
                    run_id: 1,
                    auto_apply: true
                },
                true
            ),
            "applied"
        );
        // Irreversible kinds queue even in auto-apply mode.
        assert_eq!(
            initial_status(
                &Actor::Agent {
                    run_id: 1,
                    auto_apply: true
                },
                false
            ),
            "proposed"
        );
        assert_eq!(
            initial_status(
                &Actor::Agent {
                    run_id: 1,
                    auto_apply: false
                },
                true
            ),
            "proposed"
        );
        assert_eq!(
            initial_status(
                &Actor::Agent {
                    run_id: 1,
                    auto_apply: false
                },
                false
            ),
            "proposed"
        );
    }
}

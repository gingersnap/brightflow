//! Action dispatch: the single execution path for every curation operation.
//!
//! `POST /api/actions` is idempotent on `request_id`. Human actions apply
//! immediately (undo available); agent actions default to propose-then-
//! approve — the SAME `execute_action` runs on approval.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use brightflow_engine::enrichment::{
    centroid_fingerprint, ClusteringArtifact, LabelCentroidsArtifact, ARTIFACT_VERSION,
};

use crate::actions::types::{
    Action, ActionLogEntry, ActionManifestEntry, ActionRequest, ActionResponse, ActionStatus,
    BulkApproveFailure, BulkApproveResponse, PendingCount, SuppressKind, UndoOp, ACTION_KINDS,
};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Who performed an action. The agent runner (3C) passes `Agent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    Human,
    Agent { run_id: i64 },
}

impl Actor {
    fn type_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent { .. } => "agent",
        }
    }

    fn agent_run_id(self) -> Option<i64> {
        match self {
            Self::Human => None,
            Self::Agent { run_id } => Some(run_id),
        }
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

/// `POST /api/actions` — dispatch an action as a human actor.
pub async fn dispatch(
    State(state): State<AppState>,
    Json(req): Json<ActionRequest>,
) -> AppResult<Json<ActionResponse>> {
    let response = dispatch_action(&state, req.action, &req.request_id, Actor::Human).await?;
    Ok(Json(response))
}

/// Shared dispatch used by the REST endpoint (human) and the agent runner.
/// Agent actions are recorded as `proposed` and NOT executed here.
pub async fn dispatch_action(
    state: &AppState,
    action: Action,
    request_id: &str,
    actor: Actor,
) -> AppResult<ActionResponse> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let store = std::sync::Arc::clone(store);
    let params_json = serde_json::to_string(&action)
        .map_err(|e| AppError::Internal(format!("action serialize: {e}")))?;

    let initial_status = match actor {
        Actor::Human => "applied",
        Actor::Agent { .. } => "proposed",
    };
    let inserted = store
        .db()
        .insert_action(
            request_id,
            actor.type_str(),
            actor.agent_run_id(),
            action.kind(),
            &params_json,
            initial_status,
            now_epoch(),
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

    if matches!(actor, Actor::Agent { .. }) {
        return Ok(ActionResponse {
            log_id: row.id,
            status: ActionStatus::Proposed,
            result: json!({ "proposed": true }),
        });
    }

    execute_and_record(state, row.id, action).await
}

/// Execute an action and persist result/undo onto its log row.
async fn execute_and_record(
    state: &AppState,
    log_id: i64,
    action: Action,
) -> AppResult<ActionResponse> {
    let store = state
        .store()
        .ok_or_else(|| AppError::Internal("store unavailable".into()))?;
    let store = std::sync::Arc::clone(store);
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
                    now_epoch(),
                )
                .await?;
            Ok(ActionResponse {
                log_id,
                status: ActionStatus::Applied,
                result,
            })
        },
        Err(e) => {
            let msg = e.to_string();
            store
                .db()
                .update_action_result(
                    log_id,
                    "failed",
                    Some(&json!({ "error": msg }).to_string()),
                    None,
                    now_epoch(),
                )
                .await?;
            Ok(ActionResponse {
                log_id,
                status: ActionStatus::Failed,
                result: json!({ "error": msg }),
            })
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
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let rows = store
        .db()
        .list_actions(q.limit.unwrap_or(100).clamp(1, 500))
        .await?;
    let entries = rows
        .into_iter()
        .map(|r| ActionLogEntry {
            undoable: r.undo_json.is_some() && r.status == "applied",
            id: r.id,
            request_id: r.request_id,
            actor_type: r.actor_type,
            agent_run_id: r.agent_run_id,
            action_kind: r.action_kind,
            params: serde_json::from_str(&r.params_json).unwrap_or(serde_json::Value::Null),
            result: r
                .result_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok()),
            status: r.status,
            created_at: r.created_at,
            resolved_at: r.resolved_at,
        })
        .collect();
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
            .map(|(kind, description, undoable)| ActionManifestEntry {
                kind: (*kind).to_string(),
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
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
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
    execute_and_record(&state, id, action).await.map(Json)
}

/// Cap on how many failure details a bulk approve reports back.
const MAX_REPORTED_FAILURES: usize = 20;

/// `GET /api/actions/pending-count` — proposals awaiting review.
///
/// Separate from the feed because the feed is truncated (100 by default) while
/// a `label_documents` run can propose a thousand. Counting the visible page
/// would put a wrong number on the "approve all" button.
pub async fn pending_count(State(state): State<AppState>) -> AppResult<Json<PendingCount>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
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
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let store = std::sync::Arc::clone(store);
    let rows = store.db().list_proposed_actions().await?;

    let total = rows.len();
    let mut approved = 0usize;
    let mut failures: Vec<BulkApproveFailure> = Vec::new();

    for row in rows {
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
                if let Err(db_err) = store
                    .db()
                    .update_action_result(row.id, "failed", None, None, now_epoch())
                    .await
                {
                    tracing::warn!("could not mark action {} failed: {db_err}", row.id);
                }
                continue;
            },
        };
        match execute_and_record(&state, row.id, action).await {
            Ok(response) if response.status == ActionStatus::Applied => approved += 1,
            Ok(response) => {
                let detail = response
                    .result
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("action did not apply")
                    .to_string();
                push_failure(&mut failures, row.id, &row.action_kind, detail);
            },
            Err(e) => push_failure(&mut failures, row.id, &row.action_kind, e.to_string()),
        }
    }

    let failed = total.saturating_sub(approved);
    tracing::info!("bulk approve: {approved}/{total} applied, {failed} failed");
    Ok(Json(BulkApproveResponse {
        total,
        approved,
        failed,
        failures,
    }))
}

fn push_failure(out: &mut Vec<BulkApproveFailure>, log_id: i64, kind: &str, error: String) {
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
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
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
        .set_action_status(id, "rejected", now_epoch())
        .await?;
    Ok(Json(ActionResponse {
        log_id: id,
        status: ActionStatus::Rejected,
        result: serde_json::Value::Null,
    }))
}

/// `POST /api/actions/{id}/undo`
pub async fn undo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ActionResponse>> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let store = std::sync::Arc::clone(store);
    let row = store
        .db()
        .get_action(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("action {id} not found")))?;
    if row.status != "applied" {
        return Err(AppError::BadRequest(format!(
            "action {id} is '{}', only applied actions can be undone",
            row.status
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
    apply_undo(&state, &op).await?;
    store
        .db()
        .set_action_status(id, "undone", now_epoch())
        .await?;
    Ok(Json(ActionResponse {
        log_id: id,
        status: ActionStatus::Undone,
        result: json!({ "undone": true }),
    }))
}

// ─── Execution ────────────────────────────────────────────────────────────────

/// Execute one action. Returns (result payload, inverse op when undoable).
/// This is THE action execution path — REST, approval, and the agent runner
/// all land here.
pub async fn execute_action(
    state: &AppState,
    action: &Action,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    match action {
        Action::RenameCluster {
            source_id,
            table,
            cluster_id,
            name,
        } => {
            edit_cluster(
                state,
                source_id,
                table,
                *cluster_id,
                Some(Some(name.as_str())),
                None,
                None,
                None,
                false,
            )
            .await
        },
        Action::MergeClusters {
            source_id,
            table,
            from_cluster_id,
            into_cluster_id,
        } => {
            if from_cluster_id == into_cluster_id {
                return Err(AppError::BadRequest(
                    "cannot merge a cluster into itself".to_string(),
                ));
            }
            edit_cluster(
                state,
                source_id,
                table,
                *from_cluster_id,
                None,
                None,
                None,
                Some(Some(*into_cluster_id)),
                false,
            )
            .await
        },
        Action::MarkClusterNoise {
            source_id,
            table,
            cluster_id,
            is_noise,
        } => {
            edit_cluster(
                state,
                source_id,
                table,
                *cluster_id,
                None,
                None,
                Some(*is_noise),
                None,
                false,
            )
            .await
        },
        Action::AssignClusterLabel {
            source_id,
            table,
            cluster_id,
            label,
        } => {
            edit_cluster(
                state,
                source_id,
                table,
                *cluster_id,
                None,
                Some(Some(label.as_str())),
                None,
                None,
                true,
            )
            .await
        },
        Action::ExcludeTerm {
            source_id,
            table,
            term,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let term = term.trim().to_lowercase();
            if term.is_empty() {
                return Err(AppError::BadRequest("term must not be empty".to_string()));
            }
            store
                .db()
                .add_excluded_term(&table_id, &term, now_epoch())
                .await?;
            Ok((
                json!({ "excluded": term }),
                Some(UndoOp::RemoveExcludedTerm {
                    table_id,
                    term: term.clone(),
                }),
            ))
        },
        Action::SplitCluster {
            source_id, table, ..
        } => {
            // v1 semantics: refit with one more cluster slot. Not undoable.
            let overview =
                crate::topics::handlers::run_recluster_for_action(state, source_id, table, None)
                    .await?;
            Ok((
                json!({ "refit": true, "k": overview.k, "note": "split refits with k+1" }),
                None,
            ))
        },
        Action::Recluster {
            source_id,
            table,
            k,
            language,
            embedder,
            min_cluster_size,
            algorithm,
        } => {
            let req = crate::topics::types::ReclusterRequest {
                k: k.map(|v| v as usize),
                language: language.clone(),
                embedder: embedder.clone(),
                min_cluster_size: min_cluster_size.map(|v| v as usize),
                algorithm: algorithm.clone(),
            };
            let overview = crate::topics::handlers::run_recluster_for_action(
                state,
                source_id,
                table,
                Some(req),
            )
            .await?;
            Ok((json!({ "refit": true, "k": overview.k }), None))
        },
        Action::DismissInsight {
            source_id,
            table,
            fingerprint,
            reason,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let previous = existing_state(&store, &table_id, fingerprint).await?;
            let reason_str = match reason {
                crate::actions::types::DismissReason::Boring => "boring",
                crate::actions::types::DismissReason::Known => "known",
                crate::actions::types::DismissReason::Wrong => "wrong",
            };
            store
                .db()
                .upsert_insight_state(
                    &table_id,
                    fingerprint,
                    "dismissed",
                    Some(reason_str),
                    None,
                    now_epoch(),
                )
                .await?;
            Ok((
                json!({ "dismissed": fingerprint, "reason": reason_str }),
                Some(undo_for_state(&table_id, fingerprint, previous)),
            ))
        },
        Action::PinInsight {
            source_id,
            table,
            fingerprint,
            pinned,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let previous = existing_state(&store, &table_id, fingerprint).await?;
            if *pinned {
                store
                    .db()
                    .upsert_insight_state(&table_id, fingerprint, "pinned", None, None, now_epoch())
                    .await?;
            } else {
                store
                    .db()
                    .delete_insight_state(&table_id, fingerprint)
                    .await?;
            }
            Ok((
                json!({ "pinned": pinned }),
                Some(undo_for_state(&table_id, fingerprint, previous)),
            ))
        },
        Action::AnnotateInsight {
            source_id,
            table,
            fingerprint,
            note,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let previous = existing_state(&store, &table_id, fingerprint).await?;
            // Annotation rides on the existing state (or pins implicitly).
            let (kept_state, kept_reason) = previous
                .as_ref()
                .map_or(("pinned", None), |p| (p.0.as_str(), p.1.as_deref()));
            store
                .db()
                .upsert_insight_state(
                    &table_id,
                    fingerprint,
                    kept_state,
                    kept_reason,
                    Some(note),
                    now_epoch(),
                )
                .await?;
            Ok((
                json!({ "annotated": fingerprint }),
                Some(undo_for_state(&table_id, fingerprint, previous)),
            ))
        },
        Action::SuppressTarget {
            source_id,
            table,
            target_kind,
            target,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let kind = match target_kind {
                SuppressKind::Segment => "segment",
                SuppressKind::Column => "column",
            };
            store
                .db()
                .add_insight_suppression(&table_id, kind, target, now_epoch())
                .await?;
            Ok((
                json!({ "suppressed": target, "kind": kind }),
                Some(UndoOp::DeleteSuppression {
                    table_id,
                    kind: kind.to_string(),
                    target: target.clone(),
                }),
            ))
        },
        Action::SetKpi {
            source_id,
            table,
            column,
            is_kpi,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let semantics = store.db().get_column_semantics(&table_id).await?;
            let previous = semantics.iter().find(|r| r.column_name == *column);
            let role = previous.map_or("measure", |r| r.role.as_str()).to_string();
            let prev_is_kpi = previous.is_some_and(|r| r.is_kpi);
            store
                .db()
                .upsert_column_semantic(&table_id, column, &role, *is_kpi, None, None)
                .await?;
            Ok((
                json!({ "column": column, "isKpi": is_kpi }),
                Some(UndoOp::RestoreKpi {
                    source_id: source_id.clone(),
                    table: table.clone(),
                    column: column.clone(),
                    role,
                    is_kpi: prev_is_kpi,
                }),
            ))
        },
        Action::DefineTaxonomyCategory {
            source_id,
            table,
            name,
            description,
        } => {
            let name = name.trim();
            if name.is_empty() {
                return Err(AppError::BadRequest(
                    "category name cannot be empty".to_string(),
                ));
            }
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let previous = store
                .db()
                .get_taxonomy_category_by_name(&table_id, name)
                .await?;
            let row = store
                .db()
                .upsert_taxonomy_category(&table_id, name, description.as_deref(), now_epoch())
                .await?;
            Ok((
                json!({
                    "categoryId": row.id,
                    "name": row.name,
                    "description": row.description,
                    "created": previous.is_none(),
                }),
                Some(UndoOp::RestoreTaxonomyCategory {
                    category_id: row.id,
                    delete_row: previous.is_none(),
                    name: previous.as_ref().map(|p| p.name.clone()),
                    description: previous.and_then(|p| p.description),
                }),
            ))
        },
        Action::RenameTaxonomyCategory {
            source_id,
            table,
            category_id,
            name,
        } => {
            let name = name.trim();
            if name.is_empty() {
                return Err(AppError::BadRequest(
                    "category name cannot be empty".to_string(),
                ));
            }
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let previous = owned_category(&store, &table_id, *category_id).await?;
            // UNIQUE(table_id, name) would otherwise surface as an opaque 500.
            if let Some(clash) = store
                .db()
                .get_taxonomy_category_by_name(&table_id, name)
                .await?
            {
                if clash.id != *category_id {
                    return Err(AppError::BadRequest(format!(
                        "'{name}' is already a category in this table"
                    )));
                }
            }
            let row = store
                .db()
                .rename_taxonomy_category(*category_id, name)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("category {category_id} not found")))?;
            Ok((
                json!({ "categoryId": row.id, "name": row.name }),
                Some(UndoOp::RestoreTaxonomyCategory {
                    category_id: row.id,
                    delete_row: false,
                    name: Some(previous.name),
                    description: previous.description,
                }),
            ))
        },
        Action::DeleteTaxonomyCategory {
            source_id,
            table,
            category_id,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;
            let previous = owned_category(&store, &table_id, *category_id).await?;
            // Snapshot the labels BEFORE deleting — the FK cascade is about to
            // destroy them, and they are curated human work.
            let labels: Vec<(String, String, i64)> = store
                .db()
                .get_document_labels_for_category(*category_id)
                .await?
                .into_iter()
                .map(|l| (l.row_id, l.source, l.created_at))
                .collect();
            let deleted = store.db().delete_taxonomy_category(*category_id).await?;
            if !deleted {
                return Err(AppError::NotFound(format!(
                    "category {category_id} not found"
                )));
            }
            Ok((
                json!({ "categoryId": category_id, "labelsRemoved": labels.len() }),
                Some(UndoOp::RecreateTaxonomyCategory {
                    table_id,
                    category_id: *category_id,
                    name: previous.name,
                    description: previous.description,
                    created_at: previous.created_at,
                    labels,
                }),
            ))
        },
        Action::LabelDocument {
            source_id,
            table,
            row_id,
            categories,
        } => {
            let (store, table_id) = table_ctx(state, source_id, table).await?;

            // Resolve names -> ids against the APPROVED taxonomy. An unknown
            // name is an error, not an implicit create: the taxonomy is the
            // ratified vocabulary, and letting a labeling call invent
            // categories would route around human approval entirely.
            let mut category_ids = Vec::with_capacity(categories.len());
            for name in categories {
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                let row = store
                    .db()
                    .get_taxonomy_category_by_name(&table_id, name)
                    .await?
                    .ok_or_else(|| {
                        AppError::BadRequest(format!(
                            "'{name}' is not a category in this table's taxonomy — \
                             define it first"
                        ))
                    })?;
                category_ids.push(row.id);
            }
            category_ids.sort_unstable();
            category_ids.dedup();

            let previous: Vec<(i64, String, i64)> = store
                .db()
                .get_label_rows_for_row(&table_id, row_id)
                .await?
                .into_iter()
                .map(|l| (l.category_id, l.source, l.created_at))
                .collect();

            let written = store
                .db()
                .set_document_labels(&table_id, row_id, &category_ids, "human", now_epoch())
                .await?;
            Ok((
                json!({ "rowId": row_id, "categories": categories, "count": written.len() }),
                Some(UndoOp::RestoreDocumentLabels {
                    table_id,
                    row_id: row_id.clone(),
                    labels: previous,
                }),
            ))
        },
    }
}

/// Fetch a category, verifying it belongs to `table_id`.
///
/// The ownership check is the authorization boundary: `category_id` arrives
/// from the client while the table scope comes from the URL, so without this a
/// caller could rename or delete another table's categories by guessing ids.
async fn owned_category(
    store: &StoreHandle,
    table_id: &str,
    category_id: i64,
) -> AppResult<brightflow_store::TaxonomyCategoryRow> {
    let row = store
        .db()
        .get_taxonomy_category(category_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("category {category_id} not found")))?;
    if row.table_id != table_id {
        return Err(AppError::NotFound(format!(
            "category {category_id} not found"
        )));
    }
    Ok(row)
}

/// Apply an inverse operation.
pub async fn apply_undo(state: &AppState, op: &UndoOp) -> AppResult<()> {
    let store = state
        .store()
        .ok_or_else(|| AppError::Internal("store unavailable".into()))?;
    let store = std::sync::Arc::clone(store);
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
            if *delete_row {
                let edits = store.db().get_cluster_edits(table_id).await?;
                if let Some(row) = edits
                    .iter()
                    .find(|e| e.centroid_fingerprint == *centroid_fingerprint)
                {
                    store.db().delete_cluster_edit(row.id).await?;
                }
            } else {
                store
                    .db()
                    .upsert_cluster_edit(
                        table_id,
                        centroid_fingerprint,
                        centroid_json,
                        *cluster_id,
                        Some(custom_name.as_deref()),
                        Some(label.as_deref()),
                        Some(*is_noise),
                        Some(*merged_into),
                        now_epoch(),
                    )
                    .await?;
            }
            if *refresh_labels {
                // Best-effort: label artifact refresh needs source/table which
                // we can recover from the table id via the tables catalog.
                if let Err(e) = refresh_label_artifact_by_table_id(state, table_id).await {
                    tracing::warn!("label artifact refresh after undo failed: {e}");
                }
            }
            Ok(())
        },
        UndoOp::RemoveExcludedTerm { table_id, term } => {
            store.db().remove_excluded_term(table_id, term).await?;
            Ok(())
        },
        UndoOp::DeleteInsightState {
            table_id,
            fingerprint,
        } => {
            store
                .db()
                .delete_insight_state(table_id, fingerprint)
                .await?;
            Ok(())
        },
        UndoOp::RestoreInsightState {
            table_id,
            fingerprint,
            state: st,
            reason,
            annotation,
        } => {
            store
                .db()
                .upsert_insight_state(
                    table_id,
                    fingerprint,
                    st,
                    reason.as_deref(),
                    annotation.as_deref(),
                    now_epoch(),
                )
                .await?;
            Ok(())
        },
        UndoOp::DeleteSuppression {
            table_id,
            kind,
            target,
        } => {
            let rows = store.db().get_insight_suppressions(table_id).await?;
            if let Some(row) = rows.iter().find(|r| r.kind == *kind && r.target == *target) {
                store.db().delete_insight_suppression(row.id).await?;
            }
            Ok(())
        },
        UndoOp::RestoreKpi {
            source_id,
            table,
            column,
            role,
            is_kpi,
        } => {
            let (kpi_store, table_id) = table_ctx(state, source_id, table).await?;
            kpi_store
                .db()
                .upsert_column_semantic(&table_id, column, role, *is_kpi, None, None)
                .await?;
            Ok(())
        },
        UndoOp::RestoreTaxonomyCategory {
            category_id,
            delete_row,
            name,
            description,
        } => {
            if *delete_row {
                // Undoing a *define* deletes the category, and document_labels
                // cascade off it. Between the define and the undo, rows may have
                // been labelled — the labels are curated human work, and the
                // undo op was captured before they existed, so it has no
                // snapshot to restore them from. Refuse rather than silently
                // destroy them; `delete_taxonomy_category` is the deliberate
                // path, and it DOES snapshot.
                let labels = store
                    .db()
                    .get_document_labels_for_category(*category_id)
                    .await?;
                if !labels.is_empty() {
                    return Err(AppError::BadRequest(format!(
                        "cannot undo: {} row label(s) now use this category. Delete the \
                         category explicitly instead — that path preserves the labels for undo.",
                        labels.len()
                    )));
                }
                store.db().delete_taxonomy_category(*category_id).await?;
            } else if let Some(name) = name {
                store
                    .db()
                    .update_taxonomy_category(*category_id, name, description.as_deref())
                    .await?;
            }
            Ok(())
        },
        UndoOp::RecreateTaxonomyCategory {
            table_id,
            category_id,
            name,
            description,
            created_at,
            labels,
        } => {
            store
                .db()
                .recreate_taxonomy_category(
                    *category_id,
                    table_id,
                    name,
                    description.as_deref(),
                    *created_at,
                    labels,
                )
                .await?;
            Ok(())
        },
        UndoOp::RestoreDocumentLabels {
            table_id,
            row_id,
            labels,
        } => {
            store
                .db()
                .restore_document_labels(table_id, row_id, labels)
                .await?;
            Ok(())
        },
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

type StoreHandle = std::sync::Arc<brightflow_store::ParquetStore>;

async fn table_ctx(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<(StoreHandle, String)> {
    let store = state
        .store()
        .ok_or_else(|| AppError::BadRequest("No data store configured".to_string()))?;
    let store = std::sync::Arc::clone(store);
    let row = store
        .db()
        .get_table(source_id, table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{table}' not found")))?;
    Ok((store, row.id))
}

/// Previous insight_state (state, reason, annotation) for undo capture.
async fn existing_state(
    store: &StoreHandle,
    table_id: &str,
    fingerprint: &str,
) -> AppResult<Option<(String, Option<String>, Option<String>)>> {
    let rows = store.db().get_insight_states(table_id).await?;
    Ok(rows
        .into_iter()
        .find(|r| r.fingerprint == fingerprint)
        .map(|r| (r.state, r.reason, r.annotation)))
}

fn undo_for_state(
    table_id: &str,
    fingerprint: &str,
    previous: Option<(String, Option<String>, Option<String>)>,
) -> UndoOp {
    match previous {
        Some((state, reason, annotation)) => UndoOp::RestoreInsightState {
            table_id: table_id.to_string(),
            fingerprint: fingerprint.to_string(),
            state,
            reason,
            annotation,
        },
        None => UndoOp::DeleteInsightState {
            table_id: table_id.to_string(),
            fingerprint: fingerprint.to_string(),
        },
    }
}

/// Core cluster-edit executor: attaches the edit to the cluster's centroid
/// snapshot and captures the previous row for undo.
///
/// `Option<Option<T>>` fields are deliberate tri-states: outer None = leave
/// unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
#[allow(clippy::too_many_arguments, clippy::option_option)]
async fn edit_cluster(
    state: &AppState,
    source_id: &str,
    table: &str,
    cluster_id: i64,
    custom_name: Option<Option<&str>>,
    label: Option<Option<&str>>,
    is_noise: Option<bool>,
    merged_into: Option<Option<i64>>,
    refresh_labels: bool,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let clustering = load_clustering(state, source_id, table)?;
    let idx = usize::try_from(cluster_id)
        .ok()
        .filter(|i| *i < clustering.centroids.len())
        .ok_or_else(|| AppError::BadRequest(format!("cluster {cluster_id} out of range")))?;
    let centroid = &clustering.centroids[idx];
    let fp = centroid_fingerprint(centroid);
    let centroid_json = serde_json::to_string(centroid)
        .map_err(|e| AppError::Internal(format!("centroid serialize: {e}")))?;

    // Capture the previous row for undo
    let previous = store
        .db()
        .get_cluster_edits(&table_id)
        .await?
        .into_iter()
        .find(|e| e.centroid_fingerprint == fp);
    let undo = match &previous {
        Some(p) => UndoOp::RestoreClusterEdit {
            table_id: table_id.clone(),
            centroid_fingerprint: fp.clone(),
            centroid_json: p.centroid_json.clone(),
            cluster_id: p.cluster_id,
            custom_name: p.custom_name.clone(),
            label: p.label.clone(),
            is_noise: p.is_noise,
            merged_into: p.merged_into,
            delete_row: false,
            refresh_labels,
        },
        None => UndoOp::RestoreClusterEdit {
            table_id: table_id.clone(),
            centroid_fingerprint: fp.clone(),
            centroid_json: centroid_json.clone(),
            cluster_id: Some(cluster_id),
            custom_name: None,
            label: None,
            is_noise: false,
            merged_into: None,
            delete_row: true,
            refresh_labels,
        },
    };

    let row = store
        .db()
        .upsert_cluster_edit(
            &table_id,
            &fp,
            &centroid_json,
            Some(cluster_id),
            custom_name,
            label,
            is_noise,
            merged_into,
            now_epoch(),
        )
        .await?;

    if refresh_labels {
        refresh_label_artifact(state, source_id, table, &table_id).await?;
    }

    Ok((
        json!({
            "editId": row.id,
            "clusterId": cluster_id,
            "customName": row.custom_name,
            "label": row.label,
            "isNoise": row.is_noise,
            "mergedInto": row.merged_into,
        }),
        Some(undo),
    ))
}

fn load_clustering(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<ClusteringArtifact> {
    let root = state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::root)
        .ok_or_else(|| AppError::Internal("workspace paths unavailable".to_string()))?;
    let dir = brightflow_engine::embedding::topics_artifact_dir(&root, source_id, table);
    ClusteringArtifact::load(&dir)
        .ok()
        .filter(|c| c.artifact_version == ARTIFACT_VERSION)
        .ok_or_else(|| {
            AppError::BadRequest("no current cluster fit — run recluster first".to_string())
        })
}

/// Rebuild `labels.bin` from SQLite (source of truth): every cluster edit
/// with a label contributes its centroid; duplicate labels average.
async fn refresh_label_artifact(
    state: &AppState,
    source_id: &str,
    table: &str,
    table_id: &str,
) -> AppResult<()> {
    let store = state
        .store()
        .ok_or_else(|| AppError::Internal("store unavailable".into()))?;
    let clustering = load_clustering(state, source_id, table)?;
    let edits = store.db().get_cluster_edits(table_id).await?;

    let entries: Vec<(&str, &[f32])> = edits
        .iter()
        .filter_map(|edit| {
            let (Some(label), Some(cid), false) = (&edit.label, edit.cluster_id, edit.orphaned)
            else {
                return None;
            };
            if label.is_empty() {
                return None;
            }
            let centroid = usize::try_from(cid)
                .ok()
                .and_then(|i| clustering.centroids.get(i))?;
            Some((label.as_str(), centroid.as_slice()))
        })
        .collect();
    let centroids = average_label_centroids(entries);

    let root = state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::root)
        .ok_or_else(|| AppError::Internal("workspace paths unavailable".to_string()))?;
    let dir = brightflow_engine::embedding::topics_artifact_dir(&root, source_id, table);
    let artifact = LabelCentroidsArtifact {
        centroids,
        embedding_model_id: clustering.embedding_model_id.clone(),
    };
    artifact
        .save(&dir)
        .map_err(|e| AppError::Internal(format!("label artifact save: {e}")))?;
    Ok(())
}

/// One L2-normalized centroid per label. Duplicate labels (several clusters
/// assigned the same label) average their centroids before normalizing, so a
/// label's centroid stays comparable to row embeddings via cosine.
fn average_label_centroids<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a [f32])>,
) -> std::collections::HashMap<String, Vec<f32>> {
    let mut accum: std::collections::HashMap<String, (Vec<f32>, u32)> =
        std::collections::HashMap::new();
    for (label, centroid) in entries {
        let entry = accum
            .entry(label.to_string())
            .or_insert_with(|| (vec![0.0; centroid.len()], 0));
        for (a, v) in entry.0.iter_mut().zip(centroid.iter()) {
            *a += v;
        }
        entry.1 += 1;
    }
    accum
        .into_iter()
        .map(|(label, (mut sum, count))| {
            let mut norm = 0.0f32;
            #[allow(clippy::cast_precision_loss)]
            let count_f = count as f32;
            for v in &mut sum {
                *v /= count_f;
                norm = v.mul_add(*v, norm);
            }
            let norm = norm.sqrt().max(1e-12);
            for v in &mut sum {
                *v /= norm;
            }
            (label, sum)
        })
        .collect()
}

/// Undo path helper: recover (source_id, table) from a table id.
async fn refresh_label_artifact_by_table_id(state: &AppState, table_id: &str) -> AppResult<()> {
    let store = state
        .store()
        .ok_or_else(|| AppError::Internal("store unavailable".into()))?;
    let tables = store
        .list_tables()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    for t in tables {
        if let Ok(Some(row)) = store.db().get_table(&t.source_id, &t.name).await {
            if row.id == table_id {
                return refresh_label_artifact(state, &t.source_id, &t.name, table_id).await;
            }
        }
    }
    Err(AppError::NotFound(format!("table id {table_id} not found")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::average_label_centroids;

    #[test]
    fn duplicate_labels_average_then_normalize() {
        let a = [1.0_f32, 0.0];
        let b = [0.0_f32, 1.0];
        let out = average_label_centroids([("payments", &a[..]), ("payments", &b[..])]);
        let c = &out["payments"];
        // avg = [0.5, 0.5] → normalized = [1/√2, 1/√2]
        let expected = 1.0 / 2.0_f32.sqrt();
        assert!((c[0] - expected).abs() < 1e-6);
        assert!((c[1] - expected).abs() < 1e-6);
    }

    #[test]
    fn single_label_is_normalized() {
        let long = [3.0_f32, 4.0];
        let out = average_label_centroids([("bugs", &long[..])]);
        let c = &out["bugs"];
        assert!((c[0] - 0.6).abs() < 1e-6);
        assert!((c[1] - 0.8).abs() < 1e-6);
        let norm: f32 = c.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn empty_input_yields_empty_map() {
        assert!(average_label_centroids(std::iter::empty::<(&str, &[f32])>()).is_empty());
    }
}

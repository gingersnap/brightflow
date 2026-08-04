//! Insight-feed curation: dismiss/pin/annotate one insight, suppress a
//! target — and the state-restoring undos they capture.

use serde_json::json;

use super::{table_ctx, StoreHandle};
use crate::actions::types::{SuppressKind, UndoOp};
use crate::shared::AppResult;
use crate::state::AppState;

pub(crate) async fn execute_dismiss_insight(
    state: &AppState,
    source_id: &str,
    table: &str,
    fingerprint: &str,
    reason: &crate::actions::types::DismissReason,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
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
            chrono::Utc::now().timestamp(),
        )
        .await?;
    Ok((
        json!({ "dismissed": fingerprint, "reason": reason_str }),
        Some(undo_for_state(&table_id, fingerprint, previous)),
    ))
}

pub(crate) async fn execute_pin_insight(
    state: &AppState,
    source_id: &str,
    table: &str,
    fingerprint: &str,
    pinned: bool,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = existing_state(&store, &table_id, fingerprint).await?;
    if pinned {
        store
            .db()
            .upsert_insight_state(
                &table_id,
                fingerprint,
                "pinned",
                None,
                None,
                chrono::Utc::now().timestamp(),
            )
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
}

pub(crate) async fn execute_annotate_insight(
    state: &AppState,
    source_id: &str,
    table: &str,
    fingerprint: &str,
    note: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
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
            chrono::Utc::now().timestamp(),
        )
        .await?;
    Ok((
        json!({ "annotated": fingerprint }),
        Some(undo_for_state(&table_id, fingerprint, previous)),
    ))
}

pub(crate) async fn execute_suppress_target(
    state: &AppState,
    source_id: &str,
    table: &str,
    target_kind: &SuppressKind,
    target: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let kind = match target_kind {
        SuppressKind::Segment => "segment",
        SuppressKind::Column => "column",
    };
    store
        .db()
        .add_insight_suppression(&table_id, kind, target, chrono::Utc::now().timestamp())
        .await?;
    Ok((
        json!({ "suppressed": target, "kind": kind }),
        Some(UndoOp::DeleteSuppression {
            table_id,
            kind: kind.to_string(),
            target: target.to_string(),
        }),
    ))
}

pub(crate) async fn undo_delete_insight_state(
    state: &AppState,
    table_id: &str,
    fingerprint: &str,
) -> AppResult<()> {
    let store = state.require_store()?;
    store
        .db()
        .delete_insight_state(table_id, fingerprint)
        .await?;
    Ok(())
}

pub(crate) async fn undo_restore_insight_state(
    state: &AppState,
    table_id: &str,
    fingerprint: &str,
    insight_state: &str,
    reason: Option<&str>,
    annotation: Option<&str>,
) -> AppResult<()> {
    let store = state.require_store()?;
    store
        .db()
        .upsert_insight_state(
            table_id,
            fingerprint,
            insight_state,
            reason,
            annotation,
            chrono::Utc::now().timestamp(),
        )
        .await?;
    Ok(())
}

pub(crate) async fn undo_delete_suppression(
    state: &AppState,
    table_id: &str,
    kind: &str,
    target: &str,
) -> AppResult<()> {
    let store = state.require_store()?;
    let rows = store.db().get_insight_suppressions(table_id).await?;
    if let Some(row) = rows.iter().find(|r| r.kind == kind && r.target == target) {
        store.db().delete_insight_suppression(row.id).await?;
    }
    Ok(())
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

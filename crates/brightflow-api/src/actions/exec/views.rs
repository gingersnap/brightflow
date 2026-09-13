//! Saved-view actions: save (create or overwrite), rename, delete — and the
//! undos that remove a created view or put the previous row back.
//!
//! A view's spec is the client's own JSON; the server stores it as given
//! and checks only that the name is not blank and not already taken by
//! another view of the same table. `created_by` records the person for a
//! human actor and the run for an agent, the same spelling provenance uses.

use serde_json::json;

use super::table_ctx;
use crate::actions::types::UndoOp;
use crate::actions::Actor;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Who saved it, in the provenance spelling.
fn author(actor: &Actor) -> String {
    match actor {
        Actor::Human { user_id } => format!("user:{user_id}"),
        Actor::Agent { run_id, .. } => format!("agent:{run_id}"),
    }
}

/// The name a view may have: trimmed, non-empty, and not another view's.
async fn checked_name(
    store: &brightflow_store::ParquetStore,
    table_id: &str,
    name: &str,
    own_id: Option<&str>,
) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("a view needs a name".to_string()));
    }
    if let Some(existing) = store.db().get_saved_view_by_name(table_id, name).await? {
        if own_id != Some(existing.id.as_str()) {
            return Err(AppError::BadRequest(format!(
                "a view named '{name}' already exists for this table"
            )));
        }
    }
    Ok(name.to_string())
}

pub(crate) async fn execute_save_view(
    state: &AppState,
    actor: &Actor,
    source_id: &str,
    table: &str,
    name: &str,
    spec: &serde_json::Value,
    view_id: Option<&str>,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let now = chrono::Utc::now().timestamp();
    let spec_json = serde_json::to_string(spec)
        .map_err(|e| AppError::BadRequest(format!("view spec is not JSON: {e}")))?;

    // Overwrite: the previous row is the undo.
    if let Some(id) = view_id {
        let previous = store
            .db()
            .get_saved_view(id)
            .await?
            .filter(|v| v.table_id == table_id)
            .ok_or_else(|| AppError::NotFound(format!("view {id} not found on this table")))?;
        let name = checked_name(&store, &table_id, name, Some(id)).await?;
        let row = brightflow_store::SavedViewRow {
            name,
            spec_json,
            updated_at: now,
            ..previous.clone()
        };
        store.db().upsert_saved_view(&row).await?;
        return Ok((
            json!({ "view_id": row.id, "name": row.name, "created": false }),
            Some(UndoOp::RestoreSavedView { row: previous }),
        ));
    }

    let name = checked_name(&store, &table_id, name, None).await?;
    let row = brightflow_store::SavedViewRow {
        id: uuid::Uuid::now_v7().to_string(),
        table_id,
        name,
        kind: "explore".to_string(),
        spec_json,
        created_by: Some(author(actor)),
        created_at: now,
        updated_at: now,
    };
    store.db().upsert_saved_view(&row).await?;
    Ok((
        json!({ "view_id": row.id, "name": row.name, "created": true }),
        Some(UndoOp::DeleteSavedView { view_id: row.id }),
    ))
}

pub(crate) async fn execute_rename_view(
    state: &AppState,
    source_id: &str,
    table: &str,
    view_id: &str,
    name: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = store
        .db()
        .get_saved_view(view_id)
        .await?
        .filter(|v| v.table_id == table_id)
        .ok_or_else(|| AppError::NotFound(format!("view {view_id} not found on this table")))?;
    let name = checked_name(&store, &table_id, name, Some(view_id)).await?;
    let row = brightflow_store::SavedViewRow {
        name,
        updated_at: chrono::Utc::now().timestamp(),
        ..previous.clone()
    };
    store.db().upsert_saved_view(&row).await?;
    Ok((
        json!({ "view_id": view_id, "name": row.name }),
        Some(UndoOp::RestoreSavedView { row: previous }),
    ))
}

pub(crate) async fn execute_delete_view(
    state: &AppState,
    source_id: &str,
    table: &str,
    view_id: &str,
) -> AppResult<(serde_json::Value, Option<UndoOp>)> {
    let (store, table_id) = table_ctx(state, source_id, table).await?;
    let previous = store
        .db()
        .get_saved_view(view_id)
        .await?
        .filter(|v| v.table_id == table_id)
        .ok_or_else(|| AppError::NotFound(format!("view {view_id} not found on this table")))?;
    store.db().delete_saved_view(view_id).await?;
    Ok((
        json!({ "deleted": view_id, "name": previous.name }),
        Some(UndoOp::RestoreSavedView { row: previous }),
    ))
}

pub(crate) async fn undo_delete_saved_view(state: &AppState, view_id: &str) -> AppResult<()> {
    let store = state.require_store()?;
    store.db().delete_saved_view(view_id).await?;
    Ok(())
}

pub(crate) async fn undo_restore_saved_view(
    state: &AppState,
    row: &brightflow_store::SavedViewRow,
) -> AppResult<()> {
    let store = state.require_store()?;
    store.db().upsert_saved_view(row).await?;
    Ok(())
}

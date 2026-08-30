//! Per-domain action execution.
//!
//! Each file holds one domain's `execute_*` functions adjacent to their
//! `undo_*` inverses, so whoever edits an execution path is looking at its
//! inverse. Dispatch stays in `handlers.rs` as exhaustive one-line-per-arm
//! matches — no wildcard, so adding an `Action` or `UndoOp` variant fails to
//! compile until both sides exist.

pub(crate) mod insights;
pub(crate) mod semantics;
pub(crate) mod taxonomy;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;

pub(crate) type StoreHandle = std::sync::Arc<brightflow_store::ParquetStore>;

/// Resolve the store handle and the table's catalog id for a scoped action.
pub(crate) async fn table_ctx(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<(StoreHandle, String)> {
    let store = state.require_store()?;
    let store = std::sync::Arc::clone(store);
    let row = store
        .db()
        .get_table(source_id, table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{table}' not found")))?;
    Ok((store, row.id))
}

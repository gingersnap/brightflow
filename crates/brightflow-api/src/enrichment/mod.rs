//! Enrichment functions: stored, versioned, re-runnable derived columns
//! (`llm_prompt` and the built-in `ticket_classify` run here;
//! topic_model/classifier configs share the registry).

pub mod handlers;
pub mod health;
pub mod mentions_api;
pub mod runner;
pub mod types;
pub(crate) mod validate;
pub mod vocab;

/// Function kinds this module drives per row, in post-sync order.
pub const RUNNABLE_KINDS: [&str; 3] = ["ticket_classify", "ticket_extract", "llm_prompt"];

use crate::state::AppState;

/// Post-sync hook body.
///
/// Starts an incremental (`scope=missing`) run for each promoted runnable
/// function on the synced table, unless one is already active. Prompt edits
/// never auto-run — a new spec has an empty cache, so the user picks the
/// scope explicitly (Airtable pattern). The cache makes this new-rows-only:
/// unchanged row content is a free hit.
pub async fn post_sync(state: AppState, source_id: String, table: String) {
    let Some(store) = state.store() else {
        return;
    };
    let Ok(Some(table_row)) = store.db().get_table(&source_id, &table).await else {
        return;
    };
    let mut functions = Vec::new();
    for kind in RUNNABLE_KINDS {
        match store
            .db()
            .list_promoted_functions(&table_row.id, kind)
            .await
        {
            Ok(rows) => functions.extend(rows),
            Err(e) => {
                tracing::warn!("post-sync: listing {kind} functions for '{table}' failed: {e}");
                return;
            },
        }
    }
    for function in functions {
        match handlers::start_run_internal(&state, &function, false).await {
            Ok(run_id) => {
                tracing::info!(
                    "post-sync enrichment run {run_id} started for '{}' on {source_id}/{table}",
                    function.name
                );
            },
            Err(crate::shared::AppError::Conflict(_)) => {
                tracing::info!(
                    "post-sync: run already active for '{}' on {source_id}/{table}",
                    function.name
                );
            },
            Err(e) => {
                tracing::warn!(
                    "post-sync enrichment for '{}' on {source_id}/{table} not started: {e}",
                    function.name
                );
            },
        }
    }
}

//! Ticket enrichment functions: stored, versioned, re-runnable.
//!
//! The two built-in calls (`ticket_classify`, `ticket_extract`), their
//! vocabularies, the runner, and the read endpoints over what they
//! materialise.

pub(crate) mod display;
pub mod handlers;
pub mod health;
pub mod mentions_api;
pub mod runner;
pub mod types;
pub(crate) mod validate;
pub mod vocab;
pub mod vocabulary_api;

use brightflow_engine::enrichment::FunctionSpec;

/// Function kinds this module drives per row, in post-sync order.
pub const RUNNABLE_KINDS: [&str; 2] = ["ticket_classify", "ticket_extract"];

/// The text columns the table's classifier reads — what "the ticket text"
/// means for that table. Empty when no classifier is defined yet.
pub async fn configured_text_columns(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> Vec<String> {
    let Some(store) = state.store() else {
        return Vec::new();
    };
    let Ok(Some(table_row)) = store.db().get_table(source_id, table).await else {
        return Vec::new();
    };
    let Ok(Some(config)) = store
        .db()
        .get_promoted_function_config(&table_row.id, "ticket_classify")
        .await
    else {
        return Vec::new();
    };
    match serde_json::from_str::<FunctionSpec>(&config) {
        Ok(FunctionSpec::TicketClassify(tc)) => tc.text_columns,
        _ => Vec::new(),
    }
}

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

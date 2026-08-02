//! Post-sync auto-run: compute a Trends report after a connector sync so the
//! new-findings badge lights up without anyone pressing Run.
//!
//! **Limitation — auto-runs compute Trends only.** Review and Drivers stay
//! manual. Trends is the broadest coverage per unit of compute, and this path
//! runs unattended after every sync, so the cost of running all three on every
//! table is not worth paying for reports nobody may open. Making the report set
//! configurable per table would lift it.
//!
//! Design constraints (load-bearing):
//! - **Spawn-and-return.** The scheduler awaits the post-sync hook inline
//!   between endpoint merges — blocking here delays the sync. Everything
//!   heavier than a DashMap insert runs inside `tokio::spawn`.
//! - **Single-flight** per (source, table) via `AppState.insight_auto_inflight`.
//! - **Debounced**: at most one post-sync run per table per 10 minutes.
//! - Auto-runs never call `record_shown_insights` — "shown" means a human saw
//!   it, and decaying novelty for findings nobody looked at would silently
//!   bury them.

use crate::insights::handlers::{run_trends_core, RunTrigger};
use crate::insights::types::TrendsRequest;
use crate::state::{cache_key, AppState};

/// Minimum spacing between post-sync runs for one table.
const DEBOUNCE_SECS: i64 = 600;

/// Post-sync hook entry point. Must return fast — see module docs.
#[allow(clippy::unused_async)] // hook signature requires a future
pub async fn post_sync(state: AppState, source_id: String, table: String) {
    if state.store().is_none() {
        return;
    }
    let key = cache_key(&source_id, &table);
    // Single-flight: if an auto-run for this table is already in flight, skip.
    if state.insight_auto_inflight.contains_key(&key) {
        return;
    }
    state.insight_auto_inflight.insert(key.clone(), ());

    tokio::spawn(async move {
        run_guarded(&state, &source_id, &table).await;
        state.insight_auto_inflight.remove(&key);
    });
}

async fn run_guarded(state: &AppState, source_id: &str, table: &str) {
    let Some(store) = state.store() else { return };
    let Ok(Some(table_row)) = store.db().get_table(source_id, table).await else {
        return;
    };

    // Debounce on the last post-sync run's timestamp.
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    )
    .unwrap_or(0);
    match store
        .db()
        .latest_insight_run(&table_row.id, Some("post_sync"))
        .await
    {
        Ok(Some(last)) if now - last.computed_at < DEBOUNCE_SECS => {
            tracing::debug!(
                "post-sync insights for {source_id}/{table} debounced ({}s since last)",
                now - last.computed_at
            );
            return;
        },
        Ok(_) => {},
        Err(e) => {
            tracing::warn!("post-sync insights: reading last run failed: {e}");
        },
    }

    let req = TrendsRequest {
        source_id: source_id.to_string(),
        dataset_id: table.to_string(),
        config: crate::insights::types::EngineConfig::default(),
    };
    match run_trends_core(state, req, RunTrigger::PostSync).await {
        Ok(response) => {
            tracing::info!(
                "post-sync insights for {source_id}/{table}: {} findings in {:.0}ms",
                response.finding_count,
                response.execution_time_ms
            );
        },
        Err(e) => {
            // Errors are logged, never propagated — the sync already succeeded.
            tracing::warn!("post-sync insights for {source_id}/{table} failed: {e}");
        },
    }
}

//! What happens after a table's data has been written, in one place.
//!
//! A connector sync, a model build and an enrichment materialisation all
//! rewrite a table, and the same things should follow each of them, in the
//! same order: the table gets a detected base layer if it has none, the
//! models fed by it rebuild (feeding order, awaited), promoted enrichment
//! functions on it run incrementally (they spawn), and the insights
//! auto-run is scheduled (it spawns and debounces). Models come before
//! enrichment and insights so those two see the rebuilt tables.
//!
//! A model's own output takes the shorter sequence: its dependents are the
//! caller's concern (the walk that scheduled it already covers them, or the
//! bus asks for them explicitly), so only enrichment and insights follow.

use crate::state::AppState;

/// After a table other than a model output was written: detection,
/// dependent models, enrichment, insights.
pub async fn after_table_write(state: AppState, source_id: String, table: String) {
    detect(&state, &source_id, &table).await;
    crate::models::rebuild_dependents(&state, &source_id, &table).await;
    after_model_write(state, source_id, table).await;
}

/// After a model's output was written: enrichment and insights on it.
pub async fn after_model_write(state: AppState, source_id: String, table: String) {
    crate::enrichment::post_sync(state.clone(), source_id.clone(), table.clone()).await;
    crate::insights::auto::post_sync(state, source_id, table).await;
}

/// A table that nobody has described yet gets the detector's base layer.
async fn detect(state: &AppState, source_id: &str, table: &str) {
    let Some(store) = state.store() else { return };
    if let Err(e) =
        crate::semantics::detect::declare_detected_if_undescribed(store, source_id, table).await
    {
        tracing::warn!("detection for '{source_id}/{table}' failed: {e}");
    }
}

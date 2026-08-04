//! Curated row labels: SQLite → engine.
//!
//! This module is the store-fetch glue that lets the engine stay
//! storage-agnostic: it loads curated `document_labels` rows and hands them
//! to the engine's `align_label_targets`, so id-based row alignment stays on
//! the engine side and this module never re-implements it.

use brightflow_engine::enrichment::LabelTargets;
use polars::prelude::*;

pub use brightflow_engine::enrichment::read_row_ids;

/// Load curated labels for `table_name`, aligned with `df`'s rows.
///
/// Returns `None` when nothing is curated yet — the caller then falls back to
/// the table's own `label_names` column, preserving pre-taxonomy behaviour.
pub async fn load_label_targets(
    store: &brightflow_store::ParquetStore,
    source_id: &str,
    table_name: &str,
    df: &DataFrame,
) -> Option<LabelTargets> {
    let table_row = store.db().get_table(source_id, table_name).await.ok()??;
    let labels = store.db().get_document_labels(&table_row.id).await.ok()?;
    let pairs: Vec<(String, String)> = labels.into_iter().map(|l| (l.row_id, l.name)).collect();
    brightflow_engine::enrichment::align_label_targets(df, table_name, &pairs)
}

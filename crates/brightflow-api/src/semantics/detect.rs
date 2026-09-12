//! The detector as a producer: the engine's guess about a table, filed
//! under the `detected` layer.
//!
//! Every table that nobody has described — an upload, a web events table, a
//! custom connector without declarations — gets a base layer this way, so
//! Explore, the insights engine and the export always have a role and a
//! logical type to start from, and a person's first edit has something to
//! sit on top of. It is the lowest layer: a connector's declaration, an
//! agent's or a person's edit all outrank it. Runs over a sample of the
//! table, once per creation, and again only when asked.

use brightflow_engine::data::schema::detected_declaration;
use brightflow_store::ParquetStore;
use brightflow_types::Provenance;
use polars::prelude::*;

use crate::shared::{AppError, AppResult};

/// Rows the detector samples. Enough for the cardinality rule to settle;
/// small enough that a large table stays interactive.
const DETECT_SAMPLE_ROWS: u32 = 10_000;

/// Detect and apply the `detected` layer for a table, replacing an earlier
/// detection.
pub async fn declare_detected(store: &ParquetStore, source_id: &str, table: &str) -> AppResult<()> {
    let files = store.get_table_parquet_paths(source_id, table).await?;
    if files.is_empty() {
        return Ok(());
    }
    let table_name = table.to_string();
    let decl = tokio::task::spawn_blocking(move || -> AppResult<_> {
        let df = LazyFrame::scan_parquet_files(files.into(), ScanArgsParquet::default())?
            .limit(DETECT_SAMPLE_ROWS)
            .collect()?;
        detected_declaration(&df, &table_name).map_err(|e| AppError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| AppError::Internal(format!("detection task failed: {e}")))??;
    store
        .apply_declaration(source_id, &decl, &Provenance::detected())
        .await?;
    Ok(())
}

/// Detect only when no producer and no person has said anything about the
/// table yet. For paths that create a table repeatedly (event flushes,
/// syncs) so detection does not run on every call.
pub async fn declare_detected_if_undescribed(
    store: &ParquetStore,
    source_id: &str,
    table: &str,
) -> AppResult<bool> {
    if !store.column_opinions(source_id, table).await?.is_empty() {
        return Ok(false);
    }
    declare_detected(store, source_id, table).await?;
    Ok(true)
}

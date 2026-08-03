//! Post-sync text enrichment hook: embed freshly-synced rows in place.
//!
//! Runs inside the sync path, and enrichment failing must never fail the sync —
//! the rows already landed. A skippable condition (no config, no text columns, a
//! missing column) returns `Ok(false)`; a real failure (absent model file,
//! unconfigured embedder) returns `Err`.
//!
//! Note the invariant is held by the *caller*, not here: `lib.rs` logs the `Err`
//! and carries on to `merge_parquet`. A future caller that used `?` would break
//! it silently, so that call site is the thing to preserve.
//!
//! It also never *fits* topic artifacts. Fitting is expensive and changes how
//! every existing row is labelled, so it stays an explicit manual step (`topics
//! fit` via CLI or API); this hook only applies artifacts that already exist.

use std::path::Path;

use polars::prelude::*;
use tracing::{debug, info};

use brightflow_engine::enrichment::{enrich_with_topics, EnrichmentConfig};

/// If the table supports text enrichment, embed each row and apply any existing
/// topic / label artifacts. Writes the result back to the same parquet file.
///
/// The caller resolves the effective config (promoted topic_model function ⇒
/// enrich any table; none + no builtin ⇒ `None` ⇒ skip). This function never
/// fits artifacts — that is reserved for the manual `topics fit` flow
/// (CLI / API). When no artifacts exist yet, only the `embedding` and
/// `embedding_model_id` columns are written; topic columns stay null until
/// the first manual fit.
///
/// Returns `Ok(true)` when enrichment was applied, `Ok(false)` when the table
/// is not enrichable or required columns are missing.
pub fn maybe_enrich_parquet(
    parquet_path: &Path,
    table_name: &str,
    source_id: &str,
    workspace_root: &Path,
    maybe_config: Option<&EnrichmentConfig>,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let Some(config) = maybe_config else {
        return Ok(false);
    };
    if config.text_columns.is_empty() {
        debug!("Skipping enrichment for {table_name}: no text columns configured");
        return Ok(false);
    }

    let df = read_parquet(parquet_path)?;

    for col in &config.text_columns {
        if df.column(col).is_err() {
            debug!("Skipping enrichment for {table_name}: missing required column {col}");
            return Ok(false);
        }
    }

    let enriched = match enrich_with_topics(workspace_root, source_id, table_name, &df, config) {
        Ok(d) => d,
        Err(e) => {
            // Embedder errors (e.g. missing model file) shouldn't fail the sync.
            // The Topics tab will surface a clear "embedder not configured" message.
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                e.to_string(),
            ));
        },
    };

    write_parquet(&enriched, parquet_path)?;

    let cols_added = enriched.width().saturating_sub(df.width());
    info!(
        "Enriched {table_name}: added {cols_added} columns to {} rows",
        enriched.height()
    );

    Ok(true)
}

fn read_parquet(path: &Path) -> Result<DataFrame, Box<dyn std::error::Error + Send + Sync>> {
    let file = std::fs::File::open(path)?;
    Ok(ParquetReader::new(file).finish()?)
}

fn write_parquet(
    df: &DataFrame,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = std::fs::File::create(path)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;
    Ok(())
}

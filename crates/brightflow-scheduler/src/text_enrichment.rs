use std::path::Path;

use polars::prelude::*;
use tracing::info;

use brightflow_engine::enrichment::{
    enrich_github_issues, enrich_with_existing_model, is_enrichable, model_path,
    TextEnrichmentModel, DEFAULT_NUM_CLUSTERS,
};

/// Enrich a Parquet file with text-derived columns if the table supports it.
///
/// This function:
/// 1. Checks if the table is enrichable (has title, body, label_names)
/// 2. Loads the existing model (or fits a new one on the full store data)
/// 3. Enriches the Parquet file with predicted_label, label_confidence,
///    topic_terms, and topic_cluster columns
/// 4. Writes the enriched data back to the same Parquet path
///
/// If the table is not enrichable, this is a no-op.
pub fn maybe_enrich_parquet(
    parquet_path: &Path,
    table_name: &str,
    workspace_root: &Path,
    store_root: &Path,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    if !is_enrichable(table_name) {
        return Ok(false);
    }

    // Read the parquet file
    let df = read_parquet(parquet_path)?;

    // Verify required columns exist
    let required = brightflow_engine::enrichment::required_columns(table_name);

    for col in required {
        if df.column(col).is_err() {
            return Ok(false);
        }
    }

    // Determine model path
    let model_file = model_path(workspace_root, table_name);

    // If there's an existing model, use it to enrich just the new data.
    // Otherwise, we need the full table to fit a model — load from store.
    let (enriched_df, model) = if model_file.exists() {
        // Load existing model, enrich new data
        let model = TextEnrichmentModel::load(&model_file)?;
        let enriched = enrich_with_existing_model(&df, &model)?;
        (enriched, model)
    } else {
        // Need to fit from scratch — use the new data + existing store data
        let full_df = load_full_table(store_root, table_name, &df)?;
        let (_, model) = enrich_github_issues(&full_df, None, DEFAULT_NUM_CLUSTERS)?;

        // Now enrich just the new data with the fitted model
        let enriched = enrich_with_existing_model(&df, &model)?;
        (enriched, model)
    };

    // Save model
    if let Some(parent) = model_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    model.save(&model_file)?;

    // Write enriched data back to the parquet file
    write_parquet(&enriched_df, parquet_path)?;

    let cols_added = enriched_df.width() - df.width();
    info!(
        "Enriched {table_name}: added {cols_added} columns to {} rows",
        enriched_df.height()
    );

    Ok(true)
}

/// Load the full table data from the store, combining with new data.
fn load_full_table(
    store_root: &Path,
    table_name: &str,
    new_data: &DataFrame,
) -> Result<DataFrame, Box<dyn std::error::Error + Send + Sync>> {
    let table_dir = store_root.join(table_name);

    if !table_dir.exists() {
        // No existing data — just use the new data
        return Ok(new_data.clone());
    }

    // Read all existing parquet files
    let mut frames = vec![new_data.clone()];

    for dir_entry in std::fs::read_dir(&table_dir)? {
        let path = dir_entry?.path();
        if path.extension().is_some_and(|ext| ext == "parquet") {
            if let Ok(existing) = read_parquet(&path) {
                frames.push(existing);
            }
        }
    }

    if frames.len() == 1 {
        return Ok(frames.into_iter().next().unwrap_or_default());
    }

    // Stack all frames vertically
    let mut combined = frames.remove(0);
    for frame in &frames {
        combined.vstack_mut(frame)?;
    }
    combined.rechunk_mut();
    Ok(combined)
}

fn read_parquet(path: &Path) -> Result<DataFrame, Box<dyn std::error::Error + Send + Sync>> {
    let file = std::fs::File::open(path)?;
    let df = ParquetReader::new(file).finish()?;
    Ok(df)
}

fn write_parquet(
    df: &DataFrame,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = std::fs::File::create(path)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;
    Ok(())
}

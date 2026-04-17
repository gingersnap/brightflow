use std::path::{Path, PathBuf};

use polars::prelude::*;

// Re-export for downstream convenience
pub use crate::nlp::polars::enrichment::{
    enrich_dataframe, enrich_github_issues, EnrichmentConfig, TextEnrichmentModel,
};

/// Tables that support text enrichment and their required columns.
pub const ENRICHABLE_TABLES: &[(&str, &[&str])] = &[
    ("issues", &["title", "body", "label_names"]),
    ("pull_requests", &["title", "body", "label_names"]),
];

/// Number of topic clusters for k-means.
pub const DEFAULT_NUM_CLUSTERS: usize = 10;

/// Check if a table name supports text enrichment.
pub fn is_enrichable(table_name: &str) -> bool {
    ENRICHABLE_TABLES
        .iter()
        .any(|(name, _)| *name == table_name)
}

/// Get the required columns for an enrichable table.
pub fn required_columns(table_name: &str) -> &[&str] {
    let empty: &[&str] = &[];
    ENRICHABLE_TABLES
        .iter()
        .find(|(name, _)| *name == table_name)
        .map_or(empty, |(_, cols)| cols)
}

/// Get the model file path for a given table in the workspace.
pub fn model_path(workspace_root: &Path, table_name: &str) -> PathBuf {
    let models_dir = workspace_root.join("models");
    models_dir.join(format!("{table_name}-tfidf.bin"))
}

/// Enrich a DataFrame using an already-fitted model.
///
/// Combines title + body columns, runs enrichment, and drops the temporary column.
pub fn enrich_with_existing_model(
    df: &DataFrame,
    model: &TextEnrichmentModel,
) -> Result<DataFrame, Box<dyn std::error::Error + Send + Sync>> {
    // Combine title + body
    let title = df.column("title")?.as_materialized_series().str()?;
    let body = df.column("body")?.as_materialized_series().str()?;

    let combined: StringChunked = title
        .into_iter()
        .zip(body)
        .map(|(t, b)| {
            let title_str = t.unwrap_or("");
            let body_str = b.unwrap_or("");
            Some(format!("{title_str} {body_str}"))
        })
        .collect();

    let mut work_df = df.clone();
    work_df.with_column(combined.with_name("_combined_text".into()).into_series())?;

    let enriched = enrich_dataframe(&work_df, "_combined_text", model, 5)?;

    let mut result = enriched;
    drop(result.drop_in_place("_combined_text"));

    Ok(result)
}

pub mod artifacts;
pub mod topic_enricher;

pub use artifacts::{
    ArtifactError, ArtifactMeta, ClusteringArtifact, LabelCentroidsArtifact, TfIdfArtifact,
};
pub use topic_enricher::{enrich_with_topics, fit_topics, FitOutcome, TopicError, DEFAULT_K};

/// Tables that support text enrichment and their required columns.
/// The first column in each list is treated as the "headline" (used for
/// representative sample titles in topic clusters).
pub const ENRICHABLE_TABLES: &[(&str, &[&str])] =
    &[("issues", &["title", "body"]), ("posts", &["text"])];

/// Check if a table name supports text enrichment.
pub fn is_enrichable(table_name: &str) -> bool {
    ENRICHABLE_TABLES
        .iter()
        .any(|(name, _)| *name == table_name)
}

/// Required columns for an enrichable table.
pub fn required_columns(table_name: &str) -> &[&str] {
    let empty: &[&str] = &[];
    ENRICHABLE_TABLES
        .iter()
        .find(|(name, _)| *name == table_name)
        .map_or(empty, |(_, cols)| cols)
}

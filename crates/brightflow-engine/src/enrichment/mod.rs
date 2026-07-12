pub mod artifacts;
pub mod config;
pub mod curation;
pub mod topic_enricher;

pub use artifacts::{
    ArtifactError, ArtifactMeta, ClusteringArtifact, LabelCentroidsArtifact, TfIdfArtifact,
    ARTIFACT_VERSION,
};
pub use config::{EnrichmentConfig, EnrichmentOverrides};
pub use curation::{
    centroid_fingerprint, reconcile_edits, EditCentroid, ReconcileOutcome, RECONCILE_MIN_COSINE,
};
pub use topic_enricher::{
    enrich_with_topics, fit_topics, FitOptions, FitOutcome, TopicError, DEFAULT_K,
};

use crate::nlp::CleaningProfile;

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

/// Cleaning profile applied before embedding, per table type.
/// The profile version is part of the effective model id, so changing a
/// table's profile lazily re-embeds its rows.
pub fn cleaning_profile_for(table_name: &str) -> CleaningProfile {
    match table_name {
        "issues" => CleaningProfile::MarkdownIssue,
        "posts" => CleaningProfile::Social,
        _ => CleaningProfile::Plain,
    }
}

pub mod artifacts;
pub mod config;
pub mod curation;
pub mod topic_enricher;

pub use artifacts::{
    ArtifactError, ArtifactMeta, ClassifierArtifact, ClusteringArtifact, LabelCentroidsArtifact,
    TfIdfArtifact, ARTIFACT_VERSION,
};
pub use config::{EnrichmentConfig, EnrichmentOverrides};
pub use curation::{
    centroid_fingerprint, reconcile_edits, EditCentroid, ReconcileOutcome, RECONCILE_MIN_COSINE,
};
pub use topic_enricher::{
    enrich_with_topics, fit_topics, parse_label_targets, read_existing_embeddings, FitOptions,
    FitOutcome, LabelTargets, Labeler, RowEmbeddings, TopicError, DEFAULT_K,
};

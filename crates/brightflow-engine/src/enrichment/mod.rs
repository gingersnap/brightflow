pub mod artifacts;
pub mod config;
pub mod curation;
pub mod function;
pub mod topic_enricher;

pub use artifacts::{
    ArtifactError, ArtifactMeta, ClassifierArtifact, ClusteringArtifact, LabelCentroidsArtifact,
    TfIdfArtifact, ARTIFACT_VERSION,
};
pub use config::{EnrichmentConfig, EnrichmentOverrides};
pub use curation::{
    centroid_fingerprint, reconcile_edits, EditCentroid, ReconcileOutcome, RECONCILE_MIN_COSINE,
};
pub use function::{
    extract_column_refs, input_hash, output_tool_schema, render_prompt, spec_hash, validate_output,
    ClassifierSpec, FunctionSpec, LlmPromptSpec, OutputField, OutputType, TopicModelSpec,
};
pub use topic_enricher::{
    english_stopwords, enrich_with_topics, fit_topics, parse_label_targets,
    read_existing_embeddings, FitOptions, FitOutcome, LabelTargets, Labeler, RowEmbeddings,
    TopicError, DEFAULT_K,
};

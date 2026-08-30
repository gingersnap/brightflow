//! Enrichment orchestration: fitted artifacts, their config, and the topic
//! enricher that applies them.

pub mod artifacts;
pub mod config;
pub mod curation;
pub mod function;
pub mod labels_io;
pub mod mentions;
pub mod ticket_classify;
pub mod topic_enricher;
pub mod vocabulary;

pub use artifacts::{
    ArtifactError, ArtifactMeta, ClassifierArtifact, ClusteringArtifact, LabelCentroidsArtifact,
    TfIdfArtifact, ARTIFACT_VERSION,
};
pub use config::{resolve_topic_config, EnrichmentConfig, EnrichmentOverrides};
pub use curation::{
    centroid_fingerprint, reconcile_edits, EditCentroid, ReconcileOutcome, RECONCILE_MIN_COSINE,
};
pub use function::{
    extract_column_refs, input_hash, output_tool_schema, render_prompt, spec_hash,
    ticket_classify_hash, ticket_extract_hash, validate_output, ClassifierSpec, FunctionSpec,
    LlmPromptSpec, OutputField, OutputType, TicketClassifySpec, TicketExtractSpec, TopicModelSpec,
    VocabEntry,
};
pub use labels_io::{align_label_targets, label_join_id_column, read_row_ids};
pub use topic_enricher::{
    build_clean_texts, build_combined_text, english_stopwords, enrich_with_topics, fit_topics,
    parse_label_targets, read_existing_embeddings, FitOptions, FitOutcome, LabelTargets, Labeler,
    RowEmbeddings, TopicError, DEFAULT_K,
};
pub use vocabulary::{
    check_cap, health, is_other, CapError, LevelHealth, VocabKind, HARD_BACKSTOP, IMPORTED_CAP,
    INDUCED_CAP, OTHER,
};

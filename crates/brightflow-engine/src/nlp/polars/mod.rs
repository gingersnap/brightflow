//! Bridge between the pure NLP primitives and Polars DataFrames.
//!
//! Kept separate from `nlp` proper so the algorithms stay testable without
//! constructing a DataFrame, and so a Polars upgrade cannot reach into the
//! numeric core.

pub mod centroids;
pub mod enrichment;
pub mod serde_utils;
pub mod transform;

pub use centroids::build_label_centroids;
pub use enrichment::{
    enrich_dataframe, enrich_github_issues, fit_enrichment_model, EnrichmentConfig,
    TextEnrichmentModel,
};
pub use transform::{
    cosine_similarity_column, nearest_label, tfidf_fit, tfidf_transform, TfIdfConfig,
};

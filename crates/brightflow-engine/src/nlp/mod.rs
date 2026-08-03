//! NLP primitives: tokenizing, vectorizing, clustering, and comparing text.
//!
//! Pure algorithms with no I/O and no Polars dependency, so the numeric
//! behaviour is unit-testable in isolation. This is the substrate the topic
//! modelling and text enrichment features are built from.

pub mod classification_metrics;
pub mod clean;
pub mod cluster_metrics;
pub mod clustering;
pub mod dense_clustering;
pub mod density;
pub mod error;
pub mod fingerprint;
pub mod linear;
pub mod near_dup;
pub mod ngrams;
pub mod reduce;
pub mod rng;
pub mod similarity;
pub mod sparse;
pub mod tfidf;
pub mod tokenizer;
pub mod vocabulary;

pub use clean::{
    clean_for_embedding, effective_model_id, CleaningProfile, CLEAN_VERSION, MIN_EMBED_TOKENS,
};
pub use clustering::{kmeans, ClusterResult};
pub use dense_clustering::{dense_cosine, kmeans_dense, DenseClusterResult};
pub use density::{default_min_cluster_size, hdbscan_dense};
pub use error::SubtextError;
pub use fingerprint::fingerprint;
pub use linear::{
    best_threshold, fit_centroid_baseline, fit_multilabel_linear, train_val_split,
    MultiLabelLinear, TrainOutcome,
};
pub use near_dup::{
    find_near_duplicates, NearDupError, NearDupGroup, DEFAULT_NEAR_DUP_THRESHOLD, MAX_NEAR_DUP_ROWS,
};
pub use ngrams::ngrams;
pub use reduce::Pca;
pub use rng::SplitMix64;
pub use similarity::{cosine, cosine_unnormalized};
pub use sparse::SparseVec;
pub use tfidf::{FittedTfIdf, TfIdf};
pub use tokenizer::{TokenSpan, Tokenizer, TokenizerPreset};
pub use vocabulary::{TokenId, Vocabulary};

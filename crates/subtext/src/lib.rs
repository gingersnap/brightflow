#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::shadow_reuse,
    clippy::return_self_not_must_use
)]

pub mod clustering;
pub mod error;
pub mod ngrams;
#[cfg(feature = "polars")]
pub mod polars;
pub mod similarity;
pub mod sparse;
pub mod tfidf;
pub mod tokenizer;
pub mod vocabulary;

pub use clustering::{kmeans, ClusterResult};
pub use error::SubtextError;
pub use ngrams::ngrams;
pub use similarity::{cosine, cosine_unnormalized};
pub use sparse::SparseVec;
pub use tfidf::{FittedTfIdf, TfIdf};
pub use tokenizer::{TokenSpan, Tokenizer, TokenizerPreset};
pub use vocabulary::{TokenId, Vocabulary};

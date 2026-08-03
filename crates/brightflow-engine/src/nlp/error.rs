//! NLP error type.

/// Errors that can occur in NLP operations
#[derive(Debug, thiserror::Error)]
pub enum SubtextError {
    /// Input was empty where non-empty input is required
    #[error("empty input: {context}")]
    EmptyInput { context: String },

    /// Indices and values length mismatch in sparse vector construction
    #[error("length mismatch: {indices_len} indices vs {values_len} values")]
    LengthMismatch {
        indices_len: usize,
        values_len: usize,
    },

    /// Indices are not sorted in sparse vector construction
    #[error("indices not sorted at position {position}: {prev} >= {current}")]
    IndicesNotSorted {
        position: usize,
        prev: u32,
        current: u32,
    },

    /// Other error
    #[error("{0}")]
    Other(String),

    /// Error from Polars
    #[error("polars: {0}")]
    Polars(#[from] polars::prelude::PolarsError),

    /// Serialization error (bincode)
    #[error("serialization: {0}")]
    Serialization(String),

    /// Column type mismatch
    #[error("column type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    /// Missing column
    #[error("column not found: {0}")]
    ColumnNotFound(String),
}

/// Result type for NLP operations
pub type Result<T> = std::result::Result<T, SubtextError>;

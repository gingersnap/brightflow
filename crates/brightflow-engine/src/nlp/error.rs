//! NLP error type: input-contract violations raised by the pure primitives
//! (sparse-vector construction, TF-IDF fitting, vocabulary alignment).
//!
//! Every variant carries the offending values, so the violation is
//! diagnosable from the message alone. Deliberately Polars-free: `nlp/`
//! promises to be unit-testable with no Polars dependency, and a
//! `#[from] PolarsError` here is what once silently broke that promise.

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
}

/// Result type for NLP operations
pub type Result<T> = std::result::Result<T, SubtextError>;

//! The analysis engine: statistics, NLP primitives, and enrichment orchestration.
//!
//! Layered bottom-up — `stats` and `nlp` are pure algorithms with no I/O, `data`
//! and `analysis` build findings on top of them, `enrichment` and `embedding`
//! persist and reuse fitted models, and `output` renders results. Nothing lower
//! depends on anything higher, which is what keeps the numeric core testable
//! without a workspace on disk.

// Allow certain pedantic lints that are too strict for statistical/NLP code:
// - cast_precision_loss: usize->f64 is common and acceptable for our data sizes
// - cast_possible_truncation: f64->i64/usize truncation is handled by algorithm design
// - cast_sign_loss: f64->usize sign loss is handled by algorithm design
// - cognitive_complexity: analysis functions are naturally complex
// - too_many_lines: statistical algorithms are inherently verbose
// - indexing_slicing: array access is bounds-checked at runtime; assertions would be redundant
// - shadow_reuse: variable shadowing with related values is a common pattern
// - return_self_not_must_use: builder patterns in NLP pipelines
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::indexing_slicing,
    clippy::shadow_reuse,
    clippy::return_self_not_must_use
)]

pub mod analysis;
pub mod data;
pub mod debug;
pub mod embedding;
pub mod enrichment;
pub mod nlp;
pub mod output;
pub mod stats;

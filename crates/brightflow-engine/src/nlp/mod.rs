//! Text primitives: tokenizing, n-grams, the term vocabulary, stopwords, a
//! seeded RNG, and pre-call language identification.
//!
//! Pure algorithms with no I/O and no Polars dependency, so the behaviour is
//! unit-testable in isolation. Text Explorer and ticket enrichment are built
//! from these.

pub mod error;
pub mod fingerprint;
pub mod language;
pub mod ngrams;
pub mod rng;
pub mod stopwords;
pub mod tokenizer;
pub mod vocabulary;

pub use error::SubtextError;
pub use fingerprint::fingerprint;
pub use language::{detect_language, MIN_DETECT_CHARS};
pub use ngrams::ngrams;
pub use rng::SplitMix64;
pub use stopwords::english_stopwords;
pub use tokenizer::{TokenSpan, Tokenizer, TokenizerPreset};
pub use vocabulary::{TokenId, Vocabulary};

//! Token to stable-id mapping, plus document frequencies for IDF.
//!
//! Carries a `generation` counter, incremented by `fit`, `add_document` and
//! `filter_extremes` — anything that can renumber ids. Each `SparseVec` records
//! the generation it was built under.
//!
//! Vectors that are built from one fitted model in a single pass share a
//! generation by construction. The counter is the hook a guard would use if
//! vectors are ever compared across a re-fit — comparing across generations is
//! meaningless, because a re-fit can renumber every id.

use std::collections::{HashMap, HashSet};

use super::error::Result;

/// A stable token identifier.
pub type TokenId = u32;

/// Maps tokens (and n-grams) to stable `u32` ids. Tracks document frequency
/// for IDF computation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Vocabulary {
    token_to_id: HashMap<String, TokenId>,
    id_to_token: Vec<String>,
    document_freq: Vec<u32>,
    num_documents: u32,
    generation: u64,
}

impl Vocabulary {
    /// Create an empty vocabulary.
    pub fn new() -> Self {
        Self {
            token_to_id: HashMap::new(),
            id_to_token: Vec::new(),
            document_freq: Vec::new(),
            num_documents: 0,
            generation: 0,
        }
    }

    /// Build vocabulary from pre-tokenized documents.
    ///
    /// Each document is a `Vec<String>` of tokens (including n-grams).
    /// Resets any existing vocabulary state.
    pub fn fit(&mut self, documents: &[Vec<String>]) {
        self.token_to_id.clear();
        self.id_to_token.clear();
        self.document_freq.clear();
        self.num_documents = 0;
        self.generation += 1;

        for doc_tokens in documents {
            let unique: HashSet<&str> = doc_tokens.iter().map(String::as_str).collect();
            for token in unique {
                let id = self.get_or_insert(token);
                self.document_freq[id as usize] += 1;
            }
            self.num_documents += 1;
        }
    }

    /// Extend vocabulary with new documents without resetting.
    ///
    /// Adds new tokens, updates document frequencies for existing ones,
    /// increments the generation counter.
    pub fn fit_extend(&mut self, documents: &[Vec<String>]) {
        self.generation += 1;

        for doc_tokens in documents {
            let unique: HashSet<&str> = doc_tokens.iter().map(String::as_str).collect();
            for token in unique {
                let id = self.get_or_insert(token);
                self.document_freq[id as usize] += 1;
            }
            self.num_documents += 1;
        }
    }

    /// Look up the id for a token, or `None` if unknown.
    pub fn get_id(&self, token: &str) -> Option<TokenId> {
        self.token_to_id.get(token).copied()
    }

    /// Look up the token string for an id, or `None` if out of range.
    pub fn get_token(&self, id: TokenId) -> Option<&str> {
        self.id_to_token.get(id as usize).map(String::as_str)
    }

    /// Number of tokens in the vocabulary.
    pub fn len(&self) -> usize {
        self.id_to_token.len()
    }

    /// Whether the vocabulary is empty.
    pub fn is_empty(&self) -> bool {
        self.id_to_token.is_empty()
    }

    /// Current generation counter.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Total number of documents seen during fit/fit_extend.
    pub fn num_documents(&self) -> u32 {
        self.num_documents
    }

    /// Document frequency for a token id.
    pub fn document_frequency(&self, id: TokenId) -> Option<u32> {
        self.document_freq.get(id as usize).copied()
    }

    /// Remove tokens outside the document-frequency band and remap IDs.
    ///
    /// - `min_df`: minimum document frequency. If < 1.0, treated as fraction of corpus.
    ///   If >= 1.0, treated as absolute count.
    /// - `max_df`: maximum document frequency. If <= 1.0, treated as fraction of corpus.
    ///   If > 1.0, treated as absolute count.
    ///
    /// Returns a mapping from old token IDs to new token IDs (only for surviving tokens).
    pub fn filter_extremes(
        &mut self,
        min_df: f32,
        max_df: f32,
    ) -> Result<HashMap<TokenId, TokenId>> {
        let n = self.num_documents as f32;
        let min_count = if min_df < 1.0 {
            (min_df * n).ceil() as u32
        } else {
            min_df as u32
        };
        let max_count = if max_df <= 1.0 {
            (max_df * n).floor() as u32
        } else {
            max_df as u32
        };

        let mut old_to_new: HashMap<TokenId, TokenId> = HashMap::new();
        let mut new_tokens: Vec<String> = Vec::new();
        let mut new_df: Vec<u32> = Vec::new();

        for (old_id, token) in self.id_to_token.iter().enumerate() {
            let df = self.document_freq[old_id];
            if df >= min_count && df <= max_count {
                let new_id = new_tokens.len() as TokenId;
                old_to_new.insert(old_id as TokenId, new_id);
                new_tokens.push(token.clone());
                new_df.push(df);
            }
        }

        self.token_to_id.clear();
        for (id, token) in new_tokens.iter().enumerate() {
            self.token_to_id.insert(token.clone(), id as TokenId);
        }
        self.id_to_token = new_tokens;
        self.document_freq = new_df;
        self.generation += 1;

        Ok(old_to_new)
    }

    /// Get or insert a token, returning its id.
    fn get_or_insert(&mut self, token: &str) -> TokenId {
        if let Some(&id) = self.token_to_id.get(token) {
            return id;
        }
        let id = self.id_to_token.len() as TokenId;
        self.token_to_id.insert(token.to_string(), id);
        self.id_to_token.push(token.to_string());
        self.document_freq.push(0);
        id
    }
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_docs(docs: &[&[&str]]) -> Vec<Vec<String>> {
        docs.iter()
            .map(|d| d.iter().map(|s| (*s).to_string()).collect())
            .collect()
    }

    #[test]
    fn test_fit_basic() {
        let mut vocab = Vocabulary::new();
        let docs = make_docs(&[
            &["hello", "world"],
            &["hello", "rust"],
            &["world", "rust", "code"],
        ]);
        vocab.fit(&docs);
        assert_eq!(vocab.len(), 4); // hello, world, rust, code
        assert_eq!(vocab.num_documents(), 3);
        assert_eq!(vocab.generation(), 1);

        // "hello" appears in 2 docs
        let hello_id = vocab.get_id("hello").unwrap();
        assert_eq!(vocab.document_frequency(hello_id), Some(2));

        // "code" appears in 1 doc
        let code_id = vocab.get_id("code").unwrap();
        assert_eq!(vocab.document_frequency(code_id), Some(1));
    }

    #[test]
    fn test_fit_extend() {
        let mut vocab = Vocabulary::new();
        let docs1 = make_docs(&[&["hello", "world"]]);
        vocab.fit(&docs1);
        assert_eq!(vocab.generation(), 1);
        assert_eq!(vocab.num_documents(), 1);

        let docs2 = make_docs(&[&["hello", "rust"], &["new", "token"]]);
        vocab.fit_extend(&docs2);
        assert_eq!(vocab.generation(), 2);
        assert_eq!(vocab.num_documents(), 3);

        // "hello" now in 2 docs
        let hello_id = vocab.get_id("hello").unwrap();
        assert_eq!(vocab.document_frequency(hello_id), Some(2));

        // "new" and "token" added
        assert!(vocab.get_id("new").is_some());
        assert!(vocab.get_id("token").is_some());
    }

    #[test]
    fn test_filter_extremes_min_df() {
        let mut vocab = Vocabulary::new();
        let docs = make_docs(&[
            &["common", "rare"],
            &["common", "other"],
            &["common", "another"],
        ]);
        vocab.fit(&docs);

        // Filter out tokens appearing in < 2 docs
        let remap = vocab.filter_extremes(2.0, 1.0).unwrap();
        // "common" (df=3) survives, "rare", "other", "another" (df=1) don't
        assert_eq!(vocab.len(), 1);
        assert!(vocab.get_id("common").is_some());
        assert!(vocab.get_id("rare").is_none());
        assert!(!remap.is_empty());
    }

    #[test]
    fn test_filter_extremes_max_df() {
        let mut vocab = Vocabulary::new();
        let docs = make_docs(&[
            &["the", "cat"],
            &["the", "dog"],
            &["the", "bird"],
            &["the", "fish"],
        ]);
        vocab.fit(&docs);

        // Filter out tokens appearing in > 75% of docs (> 3 docs)
        let _remap = vocab.filter_extremes(1.0, 0.75).unwrap();
        // "the" (df=4, 100%) filtered out; "cat","dog","bird","fish" (df=1, 25%) survive
        assert!(vocab.get_id("the").is_none());
        assert_eq!(vocab.len(), 4);
    }

    #[test]
    fn test_filter_extremes_generation() {
        let mut vocab = Vocabulary::new();
        let docs = make_docs(&[&["hello", "world"]]);
        vocab.fit(&docs);
        let gen_before = vocab.generation();
        let _remap = vocab.filter_extremes(1.0, 1.0).unwrap();
        assert_eq!(vocab.generation(), gen_before + 1);
    }

    #[test]
    fn test_get_unknown_token() {
        let vocab = Vocabulary::new();
        assert!(vocab.get_id("unknown").is_none());
    }

    #[test]
    fn test_get_token_by_id() {
        let mut vocab = Vocabulary::new();
        let docs = make_docs(&[&["hello", "world"]]);
        vocab.fit(&docs);
        let id = vocab.get_id("hello").unwrap();
        assert_eq!(vocab.get_token(id), Some("hello"));
    }

    #[test]
    fn test_empty_vocabulary() {
        let vocab = Vocabulary::new();
        assert_eq!(vocab.len(), 0);
        assert!(vocab.is_empty());
        assert_eq!(vocab.num_documents(), 0);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut vocab = Vocabulary::new();
        let docs = make_docs(&[&["hello", "world"], &["hello", "rust"]]);
        vocab.fit(&docs);

        let json = serde_json::to_string(&vocab).unwrap();
        let vocab2: Vocabulary = serde_json::from_str(&json).unwrap();

        assert_eq!(vocab.len(), vocab2.len());
        assert_eq!(vocab.generation(), vocab2.generation());
        assert_eq!(vocab.num_documents(), vocab2.num_documents());
        assert_eq!(
            vocab.get_id("hello").unwrap(),
            vocab2.get_id("hello").unwrap()
        );
    }
}

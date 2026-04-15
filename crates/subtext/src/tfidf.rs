use std::collections::HashMap;
use std::ops::RangeInclusive;

use crate::error::{Result, SubtextError};
use crate::ngrams::ngrams;
use crate::sparse::SparseVec;
use crate::tokenizer::{Tokenizer, TokenizerPreset};
use crate::vocabulary::{TokenId, Vocabulary};

/// Builder for configuring and fitting a TF-IDF model.
///
/// Uses sklearn-compatible defaults: unigrams, sublinear TF, L2 normalization,
/// smooth IDF, no min/max df filtering.
#[derive(Debug, Clone)]
pub struct TfIdf {
    preset: TokenizerPreset,
    tokenizer: Tokenizer,
    ngram_range: RangeInclusive<usize>,
    min_df: f32,
    max_df: f32,
    sublinear_tf: bool,
    normalize: bool,
}

/// Serializable configuration stored inside a fitted model.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TfIdfConfig {
    pub preset: TokenizerPreset,
    pub ngram_range_start: usize,
    pub ngram_range_end: usize,
    pub sublinear_tf: bool,
    pub normalize: bool,
}

/// A fitted TF-IDF model: vocabulary + IDF weights + config.
///
/// Use `transform()` to convert documents into sparse TF-IDF vectors.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FittedTfIdf {
    pub vocabulary: Vocabulary,
    pub idf_weights: Vec<f32>,
    pub config: TfIdfConfig,
}

impl TfIdf {
    /// Create a new TF-IDF builder with sklearn-compatible defaults.
    pub fn new() -> Self {
        Self {
            preset: TokenizerPreset::Plain,
            tokenizer: Tokenizer::plain(),
            ngram_range: 1..=1,
            min_df: 1.0,
            max_df: 1.0,
            sublinear_tf: true,
            normalize: true,
        }
    }

    /// Set tokenizer preset (rebuilds the internal tokenizer).
    pub fn preset(mut self, preset: TokenizerPreset) -> Self {
        self.preset = preset;
        self.tokenizer = preset.build();
        self
    }

    /// Set the n-gram range (e.g., `1..=2` for unigrams + bigrams).
    pub fn ngram_range(mut self, range: RangeInclusive<usize>) -> Self {
        self.ngram_range = range;
        self
    }

    /// Set minimum document frequency for vocabulary filtering.
    ///
    /// If < 1.0, treated as fraction of corpus. If >= 1.0, treated as absolute count.
    pub fn min_df(mut self, min_df: f32) -> Self {
        self.min_df = min_df;
        self
    }

    /// Set maximum document frequency for vocabulary filtering.
    ///
    /// If <= 1.0, treated as fraction of corpus. If > 1.0, treated as absolute count.
    pub fn max_df(mut self, max_df: f32) -> Self {
        self.max_df = max_df;
        self
    }

    /// Whether to use sublinear TF scaling: `1 + ln(tf)` (default: true).
    pub fn sublinear_tf(mut self, sublinear: bool) -> Self {
        self.sublinear_tf = sublinear;
        self
    }

    /// Whether to L2-normalize output vectors (default: true).
    pub fn normalize(mut self, normalize: bool) -> Self {
        self.normalize = normalize;
        self
    }

    /// Fit a TF-IDF model on a corpus.
    pub fn fit<S: AsRef<str>>(&self, documents: &[S]) -> Result<FittedTfIdf> {
        if documents.is_empty() {
            return Err(SubtextError::EmptyInput {
                context: "fit requires at least one document".to_string(),
            });
        }

        // Tokenize all documents
        let tokenized = self.tokenize_corpus(documents);

        // Build vocabulary
        let mut vocabulary = Vocabulary::new();
        vocabulary.fit(&tokenized);

        // Apply min_df / max_df filtering
        if self.min_df > 1.0 || self.max_df < 1.0 {
            let _remap = vocabulary.filter_extremes(self.min_df, self.max_df)?;
        }

        // Compute IDF weights: ln((1 + N) / (1 + df(t))) + 1
        let idf_weights = compute_idf(&vocabulary);

        Ok(FittedTfIdf {
            vocabulary,
            idf_weights,
            config: self.make_config(),
        })
    }

    /// Incrementally extend a fitted model with new documents.
    ///
    /// Updates vocabulary and IDF weights. Increments the generation counter.
    /// Note: vectors computed before this call are from a different generation
    /// and should be recomputed for meaningful comparison.
    pub fn fit_extend<S: AsRef<str>>(
        &self,
        fitted: &mut FittedTfIdf,
        documents: &[S],
    ) -> Result<()> {
        if documents.is_empty() {
            return Ok(());
        }

        let tokenized = self.tokenize_corpus(documents);
        fitted.vocabulary.fit_extend(&tokenized);

        // Re-apply filtering if needed
        if self.min_df > 1.0 || self.max_df < 1.0 {
            let _remap = fitted
                .vocabulary
                .filter_extremes(self.min_df, self.max_df)?;
        }

        // Recompute IDF
        fitted.idf_weights = compute_idf(&fitted.vocabulary);

        Ok(())
    }

    fn tokenize_corpus<S: AsRef<str>>(&self, documents: &[S]) -> Vec<Vec<String>> {
        documents
            .iter()
            .map(|doc| {
                let tokens = self.tokenizer.tokenize_to_strings(doc.as_ref());
                ngrams(&tokens, self.ngram_range.clone())
                    .map(std::borrow::Cow::into_owned)
                    .collect()
            })
            .collect()
    }

    fn make_config(&self) -> TfIdfConfig {
        TfIdfConfig {
            preset: self.preset,
            ngram_range_start: *self.ngram_range.start(),
            ngram_range_end: *self.ngram_range.end(),
            sublinear_tf: self.sublinear_tf,
            normalize: self.normalize,
        }
    }
}

impl Default for TfIdf {
    fn default() -> Self {
        Self::new()
    }
}

impl FittedTfIdf {
    /// Reconstruct the tokenizer from the stored preset.
    pub fn tokenizer(&self) -> Tokenizer {
        self.config.preset.build()
    }

    /// The n-gram range used during fitting.
    pub fn ngram_range(&self) -> RangeInclusive<usize> {
        self.config.ngram_range_start..=self.config.ngram_range_end
    }

    /// Transform a single document into a sparse TF-IDF vector.
    pub fn transform(&self, document: &str) -> SparseVec {
        let tokenizer = self.tokenizer();
        let tokens = tokenizer.tokenize_to_strings(document);
        let grams: Vec<String> = ngrams(&tokens, self.ngram_range())
            .map(std::borrow::Cow::into_owned)
            .collect();

        self.transform_tokens(&grams)
    }

    /// Extract the top N terms from a sparse TF-IDF vector, sorted by weight.
    ///
    /// Returns `(term, weight)` pairs in descending order of TF-IDF weight.
    pub fn top_terms(&self, vec: &SparseVec, n: usize) -> Vec<(String, f32)> {
        let mut pairs: Vec<(String, f32)> = vec
            .iter()
            .filter_map(|(idx, val)| {
                self.vocabulary
                    .get_token(idx)
                    .map(|term| (term.to_string(), val))
            })
            .collect();

        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pairs.truncate(n);
        pairs
    }

    /// Extract the top N term strings from a sparse TF-IDF vector.
    pub fn top_term_strings(&self, vec: &SparseVec, n: usize) -> Vec<String> {
        self.top_terms(vec, n)
            .into_iter()
            .map(|(term, _)| term)
            .collect()
    }

    /// Transform a batch of documents into sparse TF-IDF vectors.
    pub fn transform_batch<S: AsRef<str>>(&self, documents: &[S]) -> Vec<SparseVec> {
        let tokenizer = self.tokenizer();
        let range = self.ngram_range();

        documents
            .iter()
            .map(|doc| {
                let tokens = tokenizer.tokenize_to_strings(doc.as_ref());
                let grams: Vec<String> = ngrams(&tokens, range.clone())
                    .map(std::borrow::Cow::into_owned)
                    .collect();
                self.transform_tokens(&grams)
            })
            .collect()
    }

    fn transform_tokens(&self, tokens: &[String]) -> SparseVec {
        if tokens.is_empty() {
            return SparseVec::empty(self.vocabulary.generation());
        }

        // Count term frequencies
        let mut tf_counts: HashMap<TokenId, u32> = HashMap::new();
        for token in tokens {
            if let Some(id) = self.vocabulary.get_id(token) {
                *tf_counts.entry(id).or_insert(0) += 1;
            }
        }

        if tf_counts.is_empty() {
            return SparseVec::empty(self.vocabulary.generation());
        }

        // Build sorted (index, tfidf_value) pairs
        let mut pairs: Vec<(u32, f32)> = tf_counts
            .into_iter()
            .filter_map(|(id, count)| {
                let idf = self.idf_weights.get(id as usize).copied().unwrap_or(0.0);

                let tf = if self.config.sublinear_tf {
                    1.0 + (count as f32).ln()
                } else {
                    count as f32
                };

                let tfidf = tf * idf;
                if tfidf == 0.0 {
                    None
                } else {
                    Some((id, tfidf))
                }
            })
            .collect();

        pairs.sort_by_key(|&(idx, _)| idx);

        let indices: Vec<u32> = pairs.iter().map(|&(idx, _)| idx).collect();
        let values: Vec<f32> = pairs.iter().map(|&(_, val)| val).collect();

        // Safe because we sorted above
        let mut vec = SparseVec::new(indices, values, self.vocabulary.generation())
            .unwrap_or_else(|_| SparseVec::empty(self.vocabulary.generation()));

        if self.config.normalize {
            vec.normalize();
        }

        vec
    }
}

/// Compute IDF weights using sklearn's smooth formula: ln((1 + N) / (1 + df(t))) + 1
fn compute_idf(vocabulary: &Vocabulary) -> Vec<f32> {
    let n = vocabulary.num_documents() as f32;
    (0..vocabulary.len())
        .map(|i| {
            let df = vocabulary.document_frequency(i as TokenId).unwrap_or(0) as f32;
            ((1.0 + n) / (1.0 + df)).ln() + 1.0
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_fit_basic() {
        let docs = vec![
            "the cat sat on the mat",
            "the dog sat on the log",
            "birds are flying high",
        ];
        let fitted = TfIdf::new().fit(&docs).unwrap();
        assert!(!fitted.vocabulary.is_empty());
        assert_eq!(fitted.idf_weights.len(), fitted.vocabulary.len());
    }

    #[test]
    fn test_fit_empty_corpus() {
        let docs: Vec<&str> = vec![];
        let result = TfIdf::new().fit(&docs);
        assert!(result.is_err());
    }

    #[test]
    fn test_transform_single_doc() {
        let docs = vec!["hello world", "hello rust", "world rust"];
        let fitted = TfIdf::new().fit(&docs).unwrap();
        let vec = fitted.transform("hello world");
        assert!(!vec.is_empty());
        // Should be L2-normalized
        assert!((vec.l2_norm() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_transform_unknown_tokens() {
        let docs = vec!["hello world"];
        let fitted = TfIdf::new().fit(&docs).unwrap();
        let vec = fitted.transform("completely unknown tokens here");
        assert!(vec.is_empty());
    }

    #[test]
    fn test_transform_empty_doc() {
        let docs = vec!["hello world"];
        let fitted = TfIdf::new().fit(&docs).unwrap();
        let vec = fitted.transform("");
        assert!(vec.is_empty());
    }

    #[test]
    fn test_sklearn_idf_formula() {
        // 3 documents: "a b", "a c", "b c"
        // df(a) = 2, df(b) = 2, df(c) = 2, N = 3
        // idf(x) = ln((1+3)/(1+2)) + 1 = ln(4/3) + 1 ≈ 0.2877 + 1 = 1.2877
        let docs = vec!["aa bb", "aa cc", "bb cc"];
        let fitted = TfIdf::new().fit(&docs).unwrap();

        let expected_idf = ((1.0 + 3.0_f32) / (1.0 + 2.0)).ln() + 1.0;
        for &idf in &fitted.idf_weights {
            assert!(
                (idf - expected_idf).abs() < 1e-5,
                "expected {expected_idf}, got {idf}"
            );
        }
    }

    #[test]
    fn test_sklearn_idf_rare_term() {
        // 3 documents: "a b", "a c", "a d"
        // df(a) = 3, df(b) = 1, df(c) = 1, df(d) = 1
        // idf(a) = ln(4/4) + 1 = 0 + 1 = 1.0
        // idf(b) = ln(4/2) + 1 = ln(2) + 1 ≈ 1.6931
        let docs = vec!["aa bb", "aa cc", "aa dd"];
        let fitted = TfIdf::new().fit(&docs).unwrap();

        let id_aa = fitted.vocabulary.get_id("aa").unwrap();
        let id_bb = fitted.vocabulary.get_id("bb").unwrap();

        let idf_aa = fitted.idf_weights[id_aa as usize];
        let idf_bb = fitted.idf_weights[id_bb as usize];

        let expected_aa = ((1.0 + 3.0_f32) / (1.0 + 3.0)).ln() + 1.0; // ln(1) + 1 = 1.0
        let expected_bb = ((1.0 + 3.0_f32) / (1.0 + 1.0)).ln() + 1.0; // ln(2) + 1

        assert!(
            (idf_aa - expected_aa).abs() < 1e-5,
            "idf(aa): expected {expected_aa}, got {idf_aa}"
        );
        assert!(
            (idf_bb - expected_bb).abs() < 1e-5,
            "idf(bb): expected {expected_bb}, got {idf_bb}"
        );
    }

    #[test]
    fn test_sublinear_tf() {
        // With sublinear_tf, tf(t) = 1 + ln(raw_tf) when raw_tf > 0
        let docs = vec!["word word word other"];
        let fitted = TfIdf::new()
            .sublinear_tf(true)
            .normalize(false)
            .fit(&docs)
            .unwrap();
        let vec_raw = fitted.transform("word word word other");

        let id_word = fitted.vocabulary.get_id("word").unwrap();
        let id_other = fitted.vocabulary.get_id("other").unwrap();

        let val_word = vec_raw
            .iter()
            .find(|&(i, _)| i == id_word)
            .map_or(0.0, |(_, v)| v);
        let val_other = vec_raw
            .iter()
            .find(|&(i, _)| i == id_other)
            .map_or(0.0, |(_, v)| v);

        // Both have same IDF (single doc corpus). With sublinear TF:
        // word: tf = 1+ln(3) ≈ 2.099, other: tf = 1+ln(1) = 1.0
        let ratio = val_word / val_other;
        let expected_ratio = (1.0 + 3.0_f32.ln()) / (1.0 + 1.0_f32.ln());
        assert!(
            (ratio - expected_ratio).abs() < 1e-4,
            "ratio: expected {expected_ratio}, got {ratio}"
        );

        // Also verify normalized version has norm 1
        let fitted_norm = TfIdf::new().sublinear_tf(true).fit(&docs).unwrap();
        let vec_norm = fitted_norm.transform("word word word other");
        assert!((vec_norm.l2_norm() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_no_sublinear_tf() {
        let docs = vec!["word word word other"];
        let fitted = TfIdf::new()
            .sublinear_tf(false)
            .normalize(false)
            .fit(&docs)
            .unwrap();
        let vec = fitted.transform("word word word other");

        let id_word = fitted.vocabulary.get_id("word").unwrap();
        let id_other = fitted.vocabulary.get_id("other").unwrap();

        let val_word = vec
            .iter()
            .find(|&(i, _)| i == id_word)
            .map_or(0.0, |(_, v)| v);
        let val_other = vec
            .iter()
            .find(|&(i, _)| i == id_other)
            .map_or(0.0, |(_, v)| v);

        // Without sublinear, ratio should be raw tf ratio = 3.0
        let ratio = val_word / val_other;
        assert!(
            (ratio - 3.0).abs() < 1e-4,
            "ratio: expected 3.0, got {ratio}"
        );
    }

    #[test]
    fn test_l2_normalization() {
        let docs = vec!["hello world", "hello rust", "world rust code"];
        let fitted = TfIdf::new().fit(&docs).unwrap();

        for doc in &docs {
            let vec = fitted.transform(doc);
            if !vec.is_empty() {
                assert!(
                    (vec.l2_norm() - 1.0).abs() < 1e-5,
                    "doc '{}' has norm {}",
                    doc,
                    vec.l2_norm()
                );
            }
        }
    }

    #[test]
    fn test_fit_extend() {
        let docs1 = vec!["hello world"];
        let builder = TfIdf::new();
        let mut fitted = builder.fit(&docs1).unwrap();
        let gen1 = fitted.vocabulary.generation();

        let docs2 = vec!["hello rust", "new tokens"];
        builder.fit_extend(&mut fitted, &docs2).unwrap();

        assert!(fitted.vocabulary.generation() > gen1);
        assert_eq!(fitted.vocabulary.num_documents(), 3);
        assert!(fitted.vocabulary.get_id("rust").is_some());
    }

    #[test]
    fn test_min_df_filtering() {
        let docs = vec!["common rare1", "common rare2", "common rare3"];
        let fitted = TfIdf::new().min_df(2.0).fit(&docs).unwrap();

        // "common" (df=3) survives, "rare*" (df=1) don't
        assert!(fitted.vocabulary.get_id("common").is_some());
        assert!(fitted.vocabulary.get_id("rare1").is_none());
    }

    #[test]
    fn test_max_df_filtering() {
        let docs = vec!["the cat", "the dog", "the bird", "the fish", "the mouse"];
        let fitted = TfIdf::new().max_df(0.5).fit(&docs).unwrap();

        // "the" (df=5, 100%) filtered. Others (df=1, 20%) survive.
        assert!(fitted.vocabulary.get_id("the").is_none());
        assert!(fitted.vocabulary.get_id("cat").is_some());
    }

    #[test]
    fn test_transform_batch() {
        let docs = vec!["hello world", "hello rust", "world rust"];
        let fitted = TfIdf::new().fit(&docs).unwrap();

        let batch = fitted.transform_batch(&docs);
        assert_eq!(batch.len(), 3);

        // Each should match individual transform
        for (i, doc) in docs.iter().enumerate() {
            let individual = fitted.transform(doc);
            assert_eq!(batch[i].indices(), individual.indices());
            for (a, b) in batch[i].values().iter().zip(individual.values().iter()) {
                assert!((a - b).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn test_bigrams() {
        let docs = vec!["memory leak detected", "null pointer exception"];
        let fitted = TfIdf::new().ngram_range(1..=2).fit(&docs).unwrap();

        // Should have both unigrams and bigrams in vocabulary
        assert!(fitted.vocabulary.get_id("memory").is_some());
        assert!(fitted.vocabulary.get_id("memory leak").is_some());
    }

    #[test]
    fn test_generation_tracking() {
        let docs = vec!["hello world"];
        let fitted = TfIdf::new().fit(&docs).unwrap();
        let vec = fitted.transform("hello world");
        assert_eq!(vec.generation(), fitted.vocabulary.generation());
    }

    #[test]
    fn test_code_aware_preset() {
        let docs = vec![
            "NullPointerException in parseJson",
            "memory leak in allocator",
        ];
        let fitted = TfIdf::new()
            .preset(TokenizerPreset::CodeAware)
            .ngram_range(1..=2)
            .fit(&docs)
            .unwrap();

        // Should contain CamelCase-split tokens
        assert!(fitted.vocabulary.get_id("null").is_some());
        assert!(fitted.vocabulary.get_id("pointer").is_some());
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_fitted_serde_roundtrip() {
        let docs = vec!["hello world", "hello rust"];
        let fitted = TfIdf::new().fit(&docs).unwrap();

        let json = serde_json::to_string(&fitted).unwrap();
        let fitted2: FittedTfIdf = serde_json::from_str(&json).unwrap();

        assert_eq!(fitted.vocabulary.len(), fitted2.vocabulary.len());
        assert_eq!(fitted.idf_weights.len(), fitted2.idf_weights.len());

        // Transform should produce same results
        let v1 = fitted.transform("hello world");
        let v2 = fitted2.transform("hello world");
        assert_eq!(v1.indices(), v2.indices());
    }
}

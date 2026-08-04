//! In-memory per-table text index for instant filtering.
//!
//! Cache freshness rides `tables.version` in SQLite: every write path already
//! bumps it (including out-of-process CLI ingests), so one cheap
//! `store.db().get_table()` per request replaces any invalidation wiring.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use polars::prelude::*;

use brightflow_engine::enrichment::english_stopwords;
use brightflow_engine::nlp::{ngrams, Tokenizer, Vocabulary};

use crate::shared::{AppError, AppResult};
use crate::state::{cache_key, AppState};

/// Tables kept in memory at once. An index costs roughly 2.2× the table's
/// text size; beyond this cap entries are evicted arbitrarily (recency is not
/// tracked — an evicted index is just rebuilt on next use).
const MAX_CACHED_TABLES: usize = 3;

/// Terms must appear in at least this many rows to enter the vocabulary
/// (kills the hapax-bigram tail, which dominates raw n-gram counts).
const MIN_TERM_DF: u32 = 2;

/// N-gram span for widget terms: unigrams + bigrams ("phrases").
const NGRAM_RANGE: std::ops::RangeInclusive<usize> = 1..=2;

/// Everything needed to serve a warm textexplore request without touching
/// Parquet: the frame for cell reads, lowered row text for filtering, and a
/// CSR term index for the words widget.
pub struct TextIndex {
    /// `tables.version` at build time; a mismatch triggers a rebuild.
    pub table_version: i64,
    pub df: DataFrame,
    pub text_columns: Vec<String>,
    /// Per row: text columns joined with spaces, lowercased. Filter target.
    pub lowered: Vec<String>,
    /// Term string ↔ id mapping for the words widget.
    pub vocab: Vocabulary,
    /// CSR of deduped term ids per row: row i's terms are
    /// `row_terms_flat[row_terms_offsets[i]..row_terms_offsets[i + 1]]`.
    pub row_terms_flat: Vec<u32>,
    pub row_terms_offsets: Vec<usize>,
    /// Doc frequency (rows containing the term) over the whole corpus,
    /// indexed by term id.
    pub corpus_df: Vec<u32>,
    pub corpus_rows: usize,
}

/// Build the topics tokenizer configuration (identical to `fit_topics`) so
/// widget terms line up with cluster top-terms.
fn topics_tokenizer() -> Tokenizer {
    Tokenizer::builder()
        .lowercase(true)
        .min_token_len(2)
        .split_camel_case(true)
        .preserve_code_tokens(true)
        .stop_words(english_stopwords())
        .build()
}

/// Join a row's non-null text cells with spaces.
fn joined_row_texts(df: &DataFrame, text_columns: &[String]) -> Vec<String> {
    let n = df.height();
    let mut per_column: Vec<Option<&StringChunked>> = Vec::with_capacity(text_columns.len());
    let series: Vec<Series> = text_columns
        .iter()
        .filter_map(|col| {
            df.column(col)
                .ok()
                .map(|c| c.as_materialized_series().clone())
        })
        .collect();
    for s in &series {
        per_column.push(s.str().ok());
    }

    let mut out = Vec::with_capacity(n);
    for row in 0..n {
        let mut joined = String::new();
        for ca in per_column.iter().flatten() {
            if let Some(v) = ca.get(row) {
                if !v.is_empty() {
                    if !joined.is_empty() {
                        joined.push(' ');
                    }
                    joined.push_str(v);
                }
            }
        }
        out.push(joined);
    }
    out
}

/// Build a [`TextIndex`] from a loaded frame. CPU-bound — call from
/// `spawn_blocking`.
pub fn build_index(df: DataFrame, table_version: i64, text_columns: Vec<String>) -> TextIndex {
    let raw = joined_row_texts(&df, &text_columns);
    let lowered: Vec<String> = raw.iter().map(|s| s.to_lowercase()).collect();

    let tokenizer = topics_tokenizer();
    // Deduped term set per row (doc-frequency counting: a term counts once
    // per row however often it repeats).
    let row_term_sets: Vec<HashSet<String>> = raw
        .iter()
        .map(|text| {
            let tokens = tokenizer.tokenize_to_strings(text);
            ngrams(&tokens, NGRAM_RANGE)
                .map(std::borrow::Cow::into_owned)
                .collect()
        })
        .collect();

    // Pass 1: corpus doc frequency; prune the df < MIN_TERM_DF tail.
    let mut df_counts: HashMap<&str, u32> = HashMap::new();
    for set in &row_term_sets {
        for term in set {
            *df_counts.entry(term.as_str()).or_insert(0) += 1;
        }
    }
    // Pass 2: per-row surviving terms, interned via Vocabulary::fit (which
    // recomputes the same doc frequencies over the pruned term space).
    let docs: Vec<Vec<String>> = row_term_sets
        .iter()
        .map(|set| {
            set.iter()
                .filter(|t| df_counts.get(t.as_str()).copied().unwrap_or(0) >= MIN_TERM_DF)
                .cloned()
                .collect()
        })
        .collect();
    let mut vocab = Vocabulary::new();
    vocab.fit(&docs);

    let mut row_terms_flat = Vec::new();
    let mut row_terms_offsets = Vec::with_capacity(docs.len() + 1);
    row_terms_offsets.push(0);
    for doc in &docs {
        for term in doc {
            if let Some(id) = vocab.get_id(term) {
                row_terms_flat.push(id);
            }
        }
        row_terms_offsets.push(row_terms_flat.len());
    }

    let corpus_df: Vec<u32> = (0..vocab.len())
        .map(|id| {
            vocab
                .document_frequency(u32::try_from(id).unwrap_or(u32::MAX))
                .unwrap_or(0)
        })
        .collect();

    let corpus_rows = df.height();
    TextIndex {
        table_version,
        df,
        text_columns,
        lowered,
        vocab,
        row_terms_flat,
        row_terms_offsets,
        corpus_df,
        corpus_rows,
    }
}

/// Text columns for a table: the enrichment config's when resolvable,
/// otherwise every String-dtype column present in the frame.
fn resolve_text_columns(
    state: &AppState,
    source_id: &str,
    table: &str,
    df: &DataFrame,
) -> Vec<String> {
    let configured = crate::topics::handlers::resolve_enrichment(state, source_id, table)
        .map(|c| c.text_columns)
        .unwrap_or_default();
    let present: Vec<String> = configured
        .into_iter()
        .filter(|col| df.column(col).is_ok_and(|c| c.dtype() == &DataType::String))
        .collect();
    if !present.is_empty() {
        return present;
    }
    df.get_columns()
        .iter()
        .filter(|c| c.dtype() == &DataType::String)
        .map(|c| c.name().to_string())
        .collect()
}

/// Return the cached index for a table, rebuilding when `tables.version`
/// moved (or nothing is cached yet).
pub async fn get_or_build(
    state: &AppState,
    source_id: &str,
    table: &str,
) -> AppResult<Arc<TextIndex>> {
    let store = state.require_store()?;
    let table_row = store
        .db()
        .get_table(source_id, table)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound(format!("table '{source_id}/{table}' not found")))?;
    let version = table_row.version;

    let key = cache_key(source_id, table);
    if let Some(entry) = state.text_indexes.get(&key) {
        if entry.table_version == version {
            return Ok(Arc::clone(entry.value()));
        }
    }

    let df = store
        .read_table(source_id, table)
        .await
        .map_err(AppError::from)?;
    let text_columns = resolve_text_columns(state, source_id, table, &df);
    if text_columns.is_empty() {
        return Err(AppError::BadRequest(format!(
            "table '{table}' has no text columns to search"
        )));
    }

    let t = std::time::Instant::now();
    let index = tokio::task::spawn_blocking(move || build_index(df, version, text_columns)).await?;
    tracing::info!(
        "Built text index for '{}/{}': {} rows, {} terms in {:?}",
        source_id,
        table,
        index.corpus_rows,
        index.vocab.len(),
        t.elapsed()
    );

    let index = Arc::new(index);
    state.text_indexes.insert(key.clone(), Arc::clone(&index));

    // Bounded cache: evict other tables beyond the cap (arbitrary victims —
    // access recency isn't tracked and the rebuild cost is acceptable).
    while state.text_indexes.len() > MAX_CACHED_TABLES {
        let victim = state
            .text_indexes
            .iter()
            .map(|e| e.key().clone())
            .find(|k| k != &key);
        match victim {
            Some(v) => {
                state.text_indexes.remove(&v);
            },
            None => break,
        }
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_df() -> DataFrame {
        df!(
            "title" => &[
                Some("Parser panic on empty input"),
                Some("Feature request: dark mode"),
                None,
                Some("Parser panic when nested"),
            ],
            "body" => &[
                Some("The parser panics with empty input files."),
                Some("Please add dark mode support."),
                Some("Just a body, no title."),
                None,
            ],
        )
        .unwrap()
    }

    #[test]
    fn builds_lowered_joined_text() {
        let index = build_index(
            sample_df(),
            7,
            vec!["title".to_string(), "body".to_string()],
        );
        assert_eq!(index.table_version, 7);
        assert_eq!(index.corpus_rows, 4);
        assert_eq!(index.lowered.len(), 4);
        assert_eq!(
            index.lowered[0],
            "parser panic on empty input the parser panics with empty input files."
        );
        // Null cells are skipped but rows still exist.
        assert_eq!(index.lowered[2], "just a body, no title.");
        assert_eq!(index.lowered[3], "parser panic when nested");
    }

    #[test]
    fn vocabulary_prunes_hapax_terms() {
        let index = build_index(
            sample_df(),
            1,
            vec!["title".to_string(), "body".to_string()],
        );
        // "parser" appears in rows 0 and 3 → kept.
        let parser_id = index.vocab.get_id("parser").unwrap();
        assert_eq!(index.corpus_df[parser_id as usize], 2);
        // "nested" appears in one row only → pruned.
        assert!(index.vocab.get_id("nested").is_none());
        // Stopwords never enter the vocabulary.
        assert!(index.vocab.get_id("the").is_none());
    }

    #[test]
    fn csr_rows_match_vocab() {
        let index = build_index(
            sample_df(),
            1,
            vec!["title".to_string(), "body".to_string()],
        );
        assert_eq!(index.row_terms_offsets.len(), 5);
        assert_eq!(
            *index.row_terms_offsets.last().unwrap(),
            index.row_terms_flat.len()
        );
        // Row 0 contains "parser"; row 1 does not.
        let parser_id = index.vocab.get_id("parser").unwrap();
        let row0 = &index.row_terms_flat[index.row_terms_offsets[0]..index.row_terms_offsets[1]];
        let row1 = &index.row_terms_flat[index.row_terms_offsets[1]..index.row_terms_offsets[2]];
        assert!(row0.contains(&parser_id));
        assert!(!row1.contains(&parser_id));
    }

    #[test]
    fn bigrams_survive_when_repeated() {
        let index = build_index(
            sample_df(),
            1,
            vec!["title".to_string(), "body".to_string()],
        );
        // "parser panic" occurs in rows 0 and 3 → bigram kept.
        let id = index.vocab.get_id("parser panic").unwrap();
        assert_eq!(index.corpus_df[id as usize], 2);
    }
}

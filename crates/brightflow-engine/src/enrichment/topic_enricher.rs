use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use polars::prelude::*;
use thiserror::Error;
use tracing::info;

use crate::embedding::{
    get_backend, sanitize_source_id, topics_artifact_dir, EmbedderBackend, EmbedderError,
};
use crate::nlp::{
    clean_for_embedding, default_min_cluster_size, effective_model_id, hdbscan_dense, kmeans_dense,
    CleaningProfile, FittedTfIdf, TfIdf, Tokenizer, TokenizerPreset,
};

use super::artifacts::{
    ArtifactError, ArtifactMeta, ClusteringArtifact, LabelCentroidsArtifact, TfIdfArtifact,
    ARTIFACT_VERSION,
};
use super::config::EnrichmentConfig;

#[derive(Debug, Error)]
pub enum TopicError {
    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),
    #[error(transparent)]
    Embedder(#[from] EmbedderError),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error("missing required column: {0}")]
    MissingColumn(String),
    #[error("nlp error: {0}")]
    Nlp(String),
}

/// Options for a topics fit.
#[derive(Debug, Clone)]
pub struct FitOptions {
    /// Number of k-means clusters.
    pub num_clusters: usize,
    /// Fit on this language only (primary subtag, e.g. "en"). Defaults to the
    /// dominant language when the table has a language column.
    pub language: Option<String>,
    /// Clustering algorithm: "kmeans" (default) or "hdbscan".
    pub algorithm: Option<String>,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            num_clusters: DEFAULT_K,
            language: None,
            algorithm: None,
        }
    }
}

/// Output of a fit run.
#[derive(Debug, Clone)]
pub struct FitOutcome {
    pub k: usize,
    pub cluster_names: Vec<String>,
    pub cluster_sizes: Vec<usize>,
    pub total_rows: usize,
    /// Rows that passed cleaning + language gates and were embedded.
    pub eligible_rows: usize,
    /// Language the fit covers, when a language column was present.
    pub language: Option<String>,
    pub has_labels: bool,
}

/// Default k-means cluster count. Tiny outlier clusters get filtered from the
/// UI, so over-clustering is preferred to under-clustering — pick high enough
/// to surface real sub-themes.
pub const DEFAULT_K: usize = 12;
/// Number of representative document titles surfaced per cluster.
/// 1 headline + 5 supporting titles gives enough signal to triangulate
/// the theme without overwhelming the card.
pub const SAMPLE_TITLES_PER_CLUSTER: usize = 6;
const NGRAM_END: usize = 2;
const TOP_TERMS_PER_CLUSTER: usize = 10;
const TOP_TERMS_PER_ROW: usize = 5;
const MIN_DF: f32 = 2.0;
/// Drop terms that appear in more than this fraction of documents (filters
/// generic stopwords like "the", "to", and corpus-specific filler like a
/// repo's own name appearing in every issue).
const MAX_DF: f32 = 0.5;
const KMEANS_MAX_ITER: usize = 30;

/// Column names probed for a per-row language code.
const LANGUAGE_COLUMNS: &[&str] = &["lang", "language"];

/// Build the English stopword set used during cluster-naming TF-IDF.
fn english_stopwords() -> std::collections::HashSet<String> {
    let mut set: std::collections::HashSet<String> = stop_words::get(stop_words::LANGUAGE::English)
        .into_iter()
        .collect();
    // A few extras common in GitHub-issue prose and contractions that
    // tokenization splits into single letters.
    for extra in [
        "ve", "re", "ll", "d", "m", "s", "t", "n", "didn", "doesn", "isn", "wasn", "wouldn",
        "couldn", "shouldn", "won", "im", "ive", "ill", "dont", "cant", "wont", "thats",
    ] {
        set.insert(extra.to_string());
    }
    set
}

/// Concatenate the configured text columns (space-separated) into a single
/// raw string per row.
fn build_combined_text(df: &DataFrame, columns: &[&str]) -> Result<Vec<String>, TopicError> {
    if columns.is_empty() {
        return Err(TopicError::MissingColumn(
            "no enrichable columns configured".to_string(),
        ));
    }

    let n = df.height();
    let mut per_column: Vec<Vec<String>> = Vec::with_capacity(columns.len());
    for col in columns {
        let values = df
            .column(col)
            .map_err(|_| TopicError::MissingColumn((*col).to_string()))?
            .as_materialized_series()
            .str()
            .map_err(|e| TopicError::Nlp(format!("{col} is not a string column: {e}")))?
            .into_iter()
            .map(|o| o.unwrap_or("").to_string())
            .collect::<Vec<_>>();
        per_column.push(values);
    }

    let mut combined: Vec<String> = Vec::with_capacity(n);
    for row in 0..n {
        let mut parts: Vec<&str> = Vec::with_capacity(columns.len());
        for col_values in &per_column {
            if let Some(v) = col_values.get(row) {
                parts.push(v.as_str());
            }
        }
        combined.push(parts.join(" "));
    }
    Ok(combined)
}

/// Combined text per row, cleaned for embedding. `None` = row is ineligible
/// (too thin after cleaning) and must not be embedded or clustered.
fn build_clean_texts(
    df: &DataFrame,
    columns: &[&str],
    profile: CleaningProfile,
) -> Result<Vec<Option<String>>, TopicError> {
    let raw = build_combined_text(df, columns)?;
    Ok(raw
        .iter()
        .map(|t| clean_for_embedding(t, profile))
        .collect())
}

// ─── Language facet ───────────────────────────────────────────────────────────

/// Find the table's language column, if any.
fn detect_language_column(df: &DataFrame) -> Option<String> {
    LANGUAGE_COLUMNS
        .iter()
        .find(|c| {
            df.column(c)
                .is_ok_and(|col| col.as_materialized_series().str().is_ok())
        })
        .map(|c| (*c).to_string())
}

/// Normalize a raw language tag to its lowercase primary subtag
/// ("en-US" → "en"). Empty/blank → None.
fn normalize_lang(raw: &str) -> Option<String> {
    let primary = raw.trim().split(['-', '_']).next()?.to_lowercase();
    if primary.is_empty() {
        None
    } else {
        Some(primary)
    }
}

/// Per-language row counts, sorted descending.
fn build_language_histogram(langs: &[Option<String>]) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for l in langs.iter().flatten() {
        *counts.entry(l.clone()).or_insert(0) += 1;
    }
    let mut hist: Vec<(String, usize)> = counts.into_iter().collect();
    hist.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    hist
}

/// Normalized per-row language values for a column.
fn read_language_values(df: &DataFrame, col: &str) -> Vec<Option<String>> {
    read_string_column(df, col).map_or_else(
        || vec![None; df.height()],
        |values| {
            values
                .iter()
                .map(|o| o.as_deref().and_then(normalize_lang))
                .collect()
        },
    )
}

/// Resolved language facet for a fit: target language + histogram.
struct LanguageFacet {
    /// Target language (explicit request or dominant), when detectable.
    target: Option<String>,
    histogram: Vec<(String, usize)>,
    /// Per-row normalized language, aligned with the DataFrame.
    row_langs: Vec<Option<String>>,
}

fn resolve_language_facet(
    df: &DataFrame,
    configured_column: Option<&str>,
    requested: Option<&str>,
) -> LanguageFacet {
    let configured = configured_column
        .filter(|c| {
            df.column(c)
                .is_ok_and(|col| col.as_materialized_series().str().is_ok())
        })
        .map(str::to_string);
    let Some(col) = configured.or_else(|| detect_language_column(df)) else {
        return LanguageFacet {
            target: None,
            histogram: Vec::new(),
            row_langs: Vec::new(),
        };
    };
    let row_langs = read_language_values(df, &col);
    let histogram = build_language_histogram(&row_langs);
    let target = requested
        .and_then(normalize_lang)
        .or_else(|| histogram.first().map(|(l, _)| l.clone()));
    LanguageFacet {
        target,
        histogram,
        row_langs,
    }
}

/// Blank out rows whose language differs from the target. Rows with no
/// language tag are kept — they are most often untagged majority-language rows.
fn apply_language_gate(cleaned: &mut [Option<String>], facet: &LanguageFacet) {
    let Some(target) = &facet.target else {
        return;
    };
    if facet.row_langs.is_empty() {
        return;
    }
    for (c, lang) in cleaned.iter_mut().zip(facet.row_langs.iter()) {
        if let Some(l) = lang {
            if l != target {
                *c = None;
            }
        }
    }
}

// ─── Embeddings ───────────────────────────────────────────────────────────────

/// Per-row optional embedding vectors, aligned with the DataFrame.
type RowEmbeddings = Vec<Option<Vec<f32>>>;

/// Read an existing `embedding` column. Returns None if absent or wrong shape.
fn read_existing_embeddings(df: &DataFrame, dim: usize) -> Option<RowEmbeddings> {
    let col = df.column("embedding").ok()?;
    let list = col.as_materialized_series().list().ok()?;
    let mut out = Vec::with_capacity(list.len());
    #[allow(clippy::explicit_into_iter_loop)]
    for opt_s in list.into_iter() {
        match opt_s {
            None => out.push(None),
            Some(s) => {
                let ca = s.f32().ok()?;
                let v: Vec<f32> = ca.into_no_null_iter().collect();
                if v.len() != dim {
                    return None;
                }
                out.push(Some(v));
            },
        }
    }
    Some(out)
}

/// Read the `embedding_model_id` column if present.
fn read_existing_model_ids(df: &DataFrame) -> Vec<Option<String>> {
    df.column("embedding_model_id")
        .ok()
        .and_then(|c| {
            c.as_materialized_series()
                .str()
                .ok()
                .map(|ca| ca.into_iter().map(|o| o.map(str::to_string)).collect())
        })
        .unwrap_or_else(|| vec![None; df.height()])
}

/// True when a vector is non-degenerate (finite, non-zero norm).
fn is_valid_embedding(v: &[f32]) -> bool {
    let mut norm_sq = 0.0_f32;
    for x in v {
        if !x.is_finite() {
            return false;
        }
        norm_sq = x.mul_add(*x, norm_sq);
    }
    norm_sq > 1e-12
}

/// Compute embeddings for eligible rows only (`cleaned[i].is_some()`).
/// Ineligible rows and failed embeds are `None` — never zero vectors, which
/// used to cluster together as a fake "empty" topic.
///
/// Reuses existing row embeddings whose `embedding_model_id` matches the
/// effective model id (base model + cleaning profile version).
fn compute_embeddings(
    df: &DataFrame,
    cleaned: &[Option<String>],
    backend: &dyn EmbedderBackend,
    model_id: &str,
) -> Result<(RowEmbeddings, usize), TopicError> {
    let n = cleaned.len();
    let existing = read_existing_embeddings(df, backend.dim());
    let existing_ids = read_existing_model_ids(df);

    let mut out: Vec<Option<Vec<f32>>> = vec![None; n];
    let mut to_embed_indices: Vec<usize> = Vec::new();

    for (i, text_opt) in cleaned.iter().enumerate() {
        if text_opt.is_none() {
            continue; // ineligible — stays None
        }
        let id_matches = existing_ids
            .get(i)
            .and_then(|o| o.as_deref())
            .is_some_and(|id| id == model_id);
        let reusable = id_matches
            .then(|| {
                existing
                    .as_ref()
                    .and_then(|ex| ex.get(i).cloned().flatten())
            })
            .flatten()
            .filter(|v| is_valid_embedding(v));
        match reusable {
            Some(v) => out[i] = Some(v),
            None => to_embed_indices.push(i),
        }
    }

    let embedded_count = to_embed_indices.len();
    if !to_embed_indices.is_empty() {
        let to_embed: Vec<String> = to_embed_indices
            .iter()
            .filter_map(|&i| cleaned[i].clone())
            .collect();
        let new_embs = backend.embed(&to_embed)?;
        for (idx_in_batch, &row_idx) in to_embed_indices.iter().enumerate() {
            if let Some(emb) = new_embs.get(idx_in_batch) {
                if is_valid_embedding(emb) {
                    out[row_idx] = Some(emb.clone());
                }
            }
        }
    }

    Ok((out, embedded_count))
}

/// Build a `List<Float32>` series from per-row optional embeddings.
/// Ineligible rows become null entries.
fn embeddings_to_series(name: &str, embeddings: &[Option<Vec<f32>>], dim: usize) -> Series {
    let mut builder = ListPrimitiveChunkedBuilder::<Float32Type>::new(
        name.into(),
        embeddings.len(),
        embeddings.len() * dim,
        DataType::Float32,
    );
    for emb in embeddings {
        match emb {
            Some(e) => builder.append_slice(e),
            None => builder.append_null(),
        }
    }
    builder.finish().into_series()
}

/// Drop legacy enrichment columns if present so we can rewrite cleanly.
fn drop_enrichment_columns(df: &mut DataFrame) {
    for col in [
        "embedding",
        "embedding_model_id",
        "topic_terms",
        "topic_cluster",
        "topic_cluster_id",
        "predicted_label",
        "confidence",
    ] {
        drop(df.drop_in_place(col));
    }
}

fn write_embedding_columns(
    df: &mut DataFrame,
    embeddings: &[Option<Vec<f32>>],
    model_id: &str,
    dim: usize,
) -> Result<(), TopicError> {
    let emb_series = embeddings_to_series("embedding", embeddings, dim);
    let ids: Vec<Option<&str>> = embeddings
        .iter()
        .map(|o| o.as_ref().map(|_| model_id))
        .collect();
    let id_series = StringChunked::new("embedding_model_id".into(), ids).into_series();
    df.with_column(emb_series)?;
    df.with_column(id_series)?;
    Ok(())
}

/// Build dense label centroids by averaging row embeddings per label.
fn build_dense_label_centroids(
    df: &DataFrame,
    embeddings: &[Option<Vec<f32>>],
    dim: usize,
) -> HashMap<String, Vec<f32>> {
    let Some(label_ca) = df
        .column("label_names")
        .ok()
        .and_then(|c| c.as_materialized_series().str().ok().cloned())
    else {
        return HashMap::new();
    };

    let mut accum: HashMap<String, (Vec<f32>, u32)> = HashMap::new();

    for (i, opt) in label_ca.into_iter().enumerate() {
        let labels = match opt {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        let Some(Some(emb)) = embeddings.get(i) else {
            continue;
        };
        for label in labels.split(',') {
            let label = label.trim();
            if label.is_empty() {
                continue;
            }
            let entry = accum
                .entry(label.to_string())
                .or_insert_with(|| (vec![0.0_f32; dim], 0));
            for (j, val) in emb.iter().enumerate() {
                entry.0[j] += val;
            }
            entry.1 += 1;
        }
    }

    accum
        .into_iter()
        .filter_map(|(label, (mut sum, count))| {
            if count == 0 {
                return None;
            }
            let count_f = count as f32;
            let mut norm_sq = 0.0_f32;
            for v in &mut sum {
                *v /= count_f;
                norm_sq = v.mul_add(*v, norm_sq);
            }
            let norm = norm_sq.sqrt();
            if norm > 0.0 {
                for v in &mut sum {
                    *v /= norm;
                }
            }
            Some((label, sum))
        })
        .collect()
}

#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .fold(0.0_f32, |acc, (x, y)| x.mul_add(*y, acc))
}

const TITLE_NAME_MAX_CHARS: usize = 70;

fn read_string_column(df: &DataFrame, col: &str) -> Option<Vec<Option<String>>> {
    df.column(col).ok().and_then(|c| {
        c.as_materialized_series()
            .str()
            .ok()
            .map(|ca| ca.into_iter().map(|o| o.map(str::to_string)).collect())
    })
}

fn truncate_title(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= TITLE_NAME_MAX_CHARS {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(TITLE_NAME_MAX_CHARS).collect();
        format!("{head}…")
    }
}

/// Pick the top-N representative document titles for one cluster, ranked by
/// similarity to the centroid. Skips trivial titles (".", "---", very short).
fn representative_titles(
    cluster_idx: usize,
    assignments: &[Option<usize>],
    centroids: &[Vec<f32>],
    embeddings: &[Option<Vec<f32>>],
    titles: Option<&[Option<String>]>,
    n: usize,
) -> Vec<String> {
    let Some(titles) = titles else {
        return Vec::new();
    };
    let Some(centroid) = centroids.get(cluster_idx) else {
        return Vec::new();
    };
    if centroid.iter().all(|&v| v == 0.0) {
        return Vec::new();
    }

    let mut candidates: Vec<(usize, f32)> = assignments
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            if c == Some(cluster_idx) {
                let v = embeddings.get(i)?.as_ref()?;
                Some((i, dot(v, centroid)))
            } else {
                None
            }
        })
        .collect();
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut out = Vec::with_capacity(n);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, _sim) in candidates {
        if out.len() >= n {
            break;
        }
        if let Some(Some(t)) = titles.get(i) {
            let trimmed = t.trim();
            if trimmed.len() > 3 && trimmed != "---" {
                let truncated = truncate_title(trimmed);
                if seen.insert(truncated.clone()) {
                    out.push(truncated);
                }
            }
        }
    }
    out
}

/// c-TF-IDF: per-cluster term scoring that highlights *distinctive* terms.
///
/// For each cluster c and term t, score(t, c) = avg_in_cluster - avg_outside.
/// Terms that are common in cluster c but rare in every other cluster bubble
/// to the top, replacing the "feature, context, session" generic-term failure
/// of plain within-cluster averaging.
fn build_ctfidf_top_terms(
    k: usize,
    assignments: &[Option<usize>],
    tfidf_vectors: &[crate::nlp::SparseVec],
    fitted: &FittedTfIdf,
    sizes: &[usize],
    top_n: usize,
) -> Vec<Vec<String>> {
    let vocab_size = fitted.vocabulary.len();
    if vocab_size == 0 || k == 0 {
        return vec![Vec::new(); k];
    }

    // Per-cluster sums (dense), and total sum across the whole corpus.
    let mut cluster_sums: Vec<Vec<f32>> = (0..k).map(|_| vec![0.0_f32; vocab_size]).collect();
    let mut total_sum: Vec<f32> = vec![0.0_f32; vocab_size];

    for (row_idx, assigned) in assignments.iter().enumerate() {
        let Some(cluster) = *assigned else {
            continue;
        };
        if cluster >= k {
            continue;
        }
        let Some(v) = tfidf_vectors.get(row_idx) else {
            continue;
        };
        for (idx, val) in v.iter() {
            let i = idx as usize;
            if i < vocab_size {
                cluster_sums[cluster][i] += val;
                total_sum[i] += val;
            }
        }
    }

    let total_n: f32 = sizes.iter().sum::<usize>() as f32;
    let mut out: Vec<Vec<String>> = Vec::with_capacity(k);
    for (c, cluster_sum) in cluster_sums.iter().enumerate().take(k) {
        let size = sizes.get(c).copied().unwrap_or(0) as f32;
        if size == 0.0 {
            out.push(Vec::new());
            continue;
        }
        let other_size = (total_n - size).max(1.0);

        let mut scored: Vec<(u32, f32)> = (0..vocab_size)
            .filter_map(|t| {
                let in_total = cluster_sum[t];
                if in_total == 0.0 {
                    return None;
                }
                let in_avg = in_total / size;
                let other_total = total_sum[t] - in_total;
                let other_avg = other_total / other_size;
                let score = in_avg - other_avg;
                if score <= 0.0 {
                    None
                } else {
                    Some((u32::try_from(t).unwrap_or(0), score))
                }
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let terms: Vec<String> = scored
            .into_iter()
            .take(top_n)
            .filter_map(|(t, _)| fitted.vocabulary.get_token(t).map(str::to_string))
            .collect();
        out.push(terms);
    }
    out
}

/// Fit the full pipeline: clean → embed → TF-IDF → dense k-means → label centroids.
/// Saves artifacts to disk and returns an enriched DataFrame.
pub fn fit_topics(
    workspace_root: &Path,
    source_id: &str,
    table_name: &str,
    df: &DataFrame,
    config: &EnrichmentConfig,
    options: &FitOptions,
) -> Result<(DataFrame, FitOutcome), TopicError> {
    let backend = get_backend(workspace_root, config.embedder)?;
    let profile = config.cleaning_profile;
    let model_id = effective_model_id(backend.model_id(), profile);
    let artifact_dir = topics_artifact_dir(workspace_root, source_id, table_name);
    let columns: Vec<&str> = config.text_columns.iter().map(String::as_str).collect();
    let columns = columns.as_slice();

    info!(
        "Fitting topics for source={} table={} (sanitized={}) k={} profile={}",
        source_id,
        table_name,
        sanitize_source_id(source_id),
        options.num_clusters,
        profile.id()
    );

    // 1. Clean + language gate. `None` rows are ineligible.
    let facet = resolve_language_facet(
        df,
        config.language_column.as_deref(),
        options.language.as_deref(),
    );
    let mut cleaned = build_clean_texts(df, columns, profile)?;
    apply_language_gate(&mut cleaned, &facet);

    // 2. Embed eligible rows (reuse existing where effective model id matches).
    let (embeddings, embedded_count) =
        compute_embeddings(df, &cleaned, backend.as_ref(), &model_id)?;
    let eligible: Vec<usize> = embeddings
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.as_ref().map(|_| i))
        .collect();
    info!(
        "Embeddings: {} eligible of {} rows ({} fresh, {} reused); language={:?}",
        eligible.len(),
        df.height(),
        embedded_count,
        eligible.len().saturating_sub(embedded_count),
        facet.target
    );
    if eligible.is_empty() {
        return Err(TopicError::Nlp(
            "no rows eligible for embedding after cleaning/language gates".to_string(),
        ));
    }

    // Cleaned text per row for TF-IDF (empty for ineligible → empty vectors).
    let texts_full: Vec<String> = cleaned
        .iter()
        .map(|o| o.clone().unwrap_or_default())
        .collect();
    let texts_eligible: Vec<String> = eligible.iter().map(|&i| texts_full[i].clone()).collect();

    // 3. Fit TF-IDF on eligible cleaned text (for top-term naming).
    let t_fit = std::time::Instant::now();
    let stopwords = english_stopwords();
    let custom_tokenizer = Tokenizer::builder()
        .lowercase(true)
        .min_token_len(2)
        .split_camel_case(true)
        .preserve_code_tokens(true)
        .stop_words(stopwords)
        .build();
    let fitted_tfidf = TfIdf::new()
        .preset(TokenizerPreset::CodeAware)
        .tokenizer(custom_tokenizer)
        .ngram_range(1..=NGRAM_END)
        .min_df(MIN_DF)
        .max_df(MAX_DF)
        .sublinear_tf(true)
        .fit(&texts_eligible)
        .map_err(|e| TopicError::Nlp(e.to_string()))?;
    info!(
        "Fitted TF-IDF: {} vocab terms in {:?}",
        fitted_tfidf.vocabulary.len(),
        t_fit.elapsed()
    );
    let t_transform = std::time::Instant::now();
    let tfidf_vectors = fitted_tfidf.transform_batch(&texts_full);
    info!("TF-IDF transform_batch in {:?}", t_transform.elapsed());

    // 4. Cluster eligible rows in embedding space.
    let compact: Vec<Vec<f32>> = eligible
        .iter()
        .filter_map(|&i| embeddings[i].clone())
        .collect();
    let algorithm = options.algorithm.as_deref().unwrap_or("kmeans").to_string();
    let t_cluster = std::time::Instant::now();
    let (cluster_result, effective_min_cluster) = if algorithm == "hdbscan" {
        let mcs = config
            .min_cluster_size
            .unwrap_or_else(|| default_min_cluster_size(compact.len()));
        (hdbscan_dense(&compact, mcs), Some(mcs))
    } else {
        let k = options.num_clusters.max(1).min(compact.len());
        (kmeans_dense(&compact, k, KMEANS_MAX_ITER), None)
    };
    let k = cluster_result.centroids.len();
    if k == 0 {
        return Err(TopicError::Nlp(
            "clustering produced no clusters (all rows classified as noise)".to_string(),
        ));
    }
    info!(
        "{algorithm} produced k={k} clusters in {} iterations (inertia {:.2}), {:?}",
        cluster_result.iterations,
        cluster_result.inertia,
        t_cluster.elapsed()
    );

    // Map compacted assignments back to full-row space.
    let mut assignments: Vec<Option<usize>> = vec![None; df.height()];
    for (compact_idx, &row_idx) in eligible.iter().enumerate() {
        assignments[row_idx] = cluster_result
            .assignments
            .get(compact_idx)
            .copied()
            .flatten();
    }

    // 5. Cluster sizes (assigned members only).
    let mut cluster_sizes: Vec<usize> = vec![0; k];
    for assigned in assignments.iter().flatten() {
        if *assigned < k {
            cluster_sizes[*assigned] += 1;
        }
    }
    info!("Cluster sizes (kmeans, post-trim): {:?}", cluster_sizes);

    // 6. Per-cluster naming:
    //    - sample_titles: representative doc titles (closest to centroid)
    //    - top_terms: c-TF-IDF distinctive terms
    //    - names: short single-line name combining both.
    let headline_col = columns.first().copied().unwrap_or("title");
    let titles = read_string_column(df, headline_col);
    let mut sample_titles: Vec<Vec<String>> = Vec::with_capacity(k);
    for cluster_idx in 0..k {
        sample_titles.push(representative_titles(
            cluster_idx,
            &assignments,
            &cluster_result.centroids,
            &embeddings,
            titles.as_deref(),
            SAMPLE_TITLES_PER_CLUSTER,
        ));
    }
    let top_terms = build_ctfidf_top_terms(
        k,
        &assignments,
        &tfidf_vectors,
        &fitted_tfidf,
        &cluster_sizes,
        TOP_TERMS_PER_CLUSTER,
    );
    let cluster_names: Vec<String> = (0..k)
        .map(|c| {
            let head = sample_titles
                .get(c)
                .and_then(|s| s.first())
                .cloned()
                .unwrap_or_default();
            let terms = top_terms.get(c).cloned().unwrap_or_default().join(", ");
            let size = cluster_sizes.get(c).copied().unwrap_or(0);
            match (head.is_empty(), terms.is_empty()) {
                (false, false) => format!("{head} — {terms}"),
                (false, true) => head,
                (true, false) => terms,
                (true, true) => format!("cluster {c} ({size} docs)"),
            }
        })
        .collect();

    // 7. Build dense label centroids.
    let t_labels = std::time::Instant::now();
    let label_centroids = build_dense_label_centroids(df, &embeddings, backend.dim());
    let has_labels = !label_centroids.is_empty();
    info!(
        "Built {} label centroids in {:?}",
        label_centroids.len(),
        t_labels.elapsed()
    );

    // 8. Persist artifacts.
    std::fs::create_dir_all(&artifact_dir).map_err(|e| ArtifactError::Io {
        path: artifact_dir.display().to_string(),
        message: e.to_string(),
    })?;

    let tfidf_artifact = TfIdfArtifact {
        fitted: fitted_tfidf.clone(),
        embedding_model_id: model_id.clone(),
    };
    tfidf_artifact.save(&artifact_dir)?;

    let clustering = ClusteringArtifact {
        centroids: cluster_result.centroids,
        names: cluster_names.clone(),
        top_terms,
        sample_titles,
        embedding_model_id: model_id.clone(),
        k,
        fitted_at: Utc::now().timestamp(),
        assign_thresholds: cluster_result.assign_thresholds,
        dim: backend.dim(),
        language: facet.target.clone(),
        language_histogram: facet.histogram.clone(),
        artifact_version: ARTIFACT_VERSION,
        algorithm: algorithm.clone(),
        min_cluster_size: effective_min_cluster,
        pca_dims: (algorithm == "hdbscan").then_some(12),
    };
    clustering.save(&artifact_dir)?;

    if has_labels {
        let labels_artifact = LabelCentroidsArtifact {
            centroids: label_centroids.clone(),
            embedding_model_id: model_id.clone(),
        };
        labels_artifact.save(&artifact_dir)?;
    }

    let total_rows = df.height();
    let meta = ArtifactMeta {
        embedding_model_id: model_id.clone(),
        k,
        fitted_at: clustering.fitted_at,
        total_rows,
        has_labels,
        language: facet.target.clone(),
        artifact_version: ARTIFACT_VERSION,
        algorithm: algorithm.clone(),
        min_cluster_size: effective_min_cluster,
        pca_dims: (algorithm == "hdbscan").then_some(12),
    };
    meta.save(&artifact_dir)?;
    info!("Saved artifacts to {}", artifact_dir.display());

    // 9. Build enriched DataFrame.
    let t_enrich = std::time::Instant::now();
    let mut work = df.clone();
    drop_enrichment_columns(&mut work);
    write_embedding_columns(&mut work, &embeddings, &model_id, backend.dim())?;

    apply_topics(
        &mut work,
        &embeddings,
        &tfidf_vectors,
        &fitted_tfidf,
        Some(&clustering),
        if has_labels {
            Some(&label_centroids)
        } else {
            None
        },
    )?;
    info!(
        "Built enriched DataFrame ({} rows, {} cols) in {:?}",
        work.height(),
        work.width(),
        t_enrich.elapsed()
    );

    Ok((
        work,
        FitOutcome {
            k,
            cluster_names,
            cluster_sizes,
            total_rows,
            eligible_rows: eligible.len(),
            language: facet.target,
            has_labels,
        },
    ))
}

/// Embed-and-assign-only: never re-fits.
///
/// Applies whatever artifacts exist on disk to the DataFrame. Always writes
/// `embedding` and `embedding_model_id`; topic columns are written only when
/// fitted artifacts of the current version and model id are present.
pub fn enrich_with_topics(
    workspace_root: &Path,
    source_id: &str,
    table_name: &str,
    df: &DataFrame,
    config: &EnrichmentConfig,
) -> Result<DataFrame, TopicError> {
    let backend = get_backend(workspace_root, config.embedder)?;
    let profile = config.cleaning_profile;
    let model_id = effective_model_id(backend.model_id(), profile);
    let artifact_dir = topics_artifact_dir(workspace_root, source_id, table_name);
    let columns: Vec<&str> = config.text_columns.iter().map(String::as_str).collect();
    let columns = columns.as_slice();

    // Apply the fitted language gate so newly synced rows in other languages
    // stay unassigned, matching fit-time behavior.
    let clustering = ClusteringArtifact::load(&artifact_dir)
        .ok()
        .filter(|a| a.embedding_model_id == model_id && a.artifact_version == ARTIFACT_VERSION);

    let facet = resolve_language_facet(
        df,
        config.language_column.as_deref(),
        clustering.as_ref().and_then(|c| c.language.as_deref()),
    );
    let mut cleaned = build_clean_texts(df, columns, profile)?;
    if clustering.is_some() {
        apply_language_gate(&mut cleaned, &facet);
    }

    let (embeddings, embedded_count) =
        compute_embeddings(df, &cleaned, backend.as_ref(), &model_id)?;
    info!(
        "Enriched {} rows ({} fresh embeddings) for {table_name}",
        embeddings.len(),
        embedded_count
    );

    // Recompute TF-IDF vectors per row only if a fitted TF-IDF artifact exists.
    let tfidf_artifact = TfIdfArtifact::load(&artifact_dir)
        .ok()
        .filter(|a| a.embedding_model_id == model_id);
    let texts_full: Vec<String> = cleaned
        .iter()
        .map(|o| o.clone().unwrap_or_default())
        .collect();
    let tfidf_vectors = tfidf_artifact
        .as_ref()
        .map(|a| a.fitted.transform_batch(&texts_full));

    let labels = LabelCentroidsArtifact::load(&artifact_dir)
        .ok()
        .filter(|a| a.embedding_model_id == model_id)
        .map(|a| a.centroids);

    let mut work = df.clone();
    drop_enrichment_columns(&mut work);
    write_embedding_columns(&mut work, &embeddings, &model_id, backend.dim())?;

    if let (Some(t_artifact), Some(t_vectors)) = (tfidf_artifact.as_ref(), tfidf_vectors.as_ref()) {
        apply_topics(
            &mut work,
            &embeddings,
            t_vectors,
            &t_artifact.fitted,
            clustering.as_ref(),
            labels.as_ref(),
        )?;
    }

    Ok(work)
}

/// Apply enrichment columns: topic_terms, topic_cluster*, predicted_label, confidence.
///
/// Rows without an embedding get null topic columns. Cluster assignment is
/// gated by the fitted per-cluster similarity thresholds — rows that don't
/// genuinely fit any cluster stay unassigned instead of being forced into
/// the least-bad one.
fn apply_topics(
    df: &mut DataFrame,
    embeddings: &[Option<Vec<f32>>],
    tfidf_vectors: &[crate::nlp::SparseVec],
    fitted: &FittedTfIdf,
    clustering: Option<&ClusteringArtifact>,
    label_centroids: Option<&HashMap<String, Vec<f32>>>,
) -> Result<(), TopicError> {
    let n = embeddings.len();

    // topic_terms
    let mut topic_terms: Vec<Option<String>> = Vec::with_capacity(n);
    for v in tfidf_vectors {
        if v.is_empty() {
            topic_terms.push(None);
        } else {
            let terms = fitted.top_term_strings(v, TOP_TERMS_PER_ROW);
            topic_terms.push(if terms.is_empty() {
                None
            } else {
                Some(terms.join(", "))
            });
        }
    }
    let topic_terms_series = StringChunked::new("topic_terms".into(), topic_terms).into_series();
    df.with_column(topic_terms_series)?;

    // topic_cluster + topic_cluster_id
    if let Some(c) = clustering {
        let mut cluster_ids: Vec<Option<i32>> = Vec::with_capacity(n);
        let mut cluster_names: Vec<Option<String>> = Vec::with_capacity(n);
        for emb_opt in embeddings {
            let Some(emb) = emb_opt else {
                cluster_ids.push(None);
                cluster_names.push(None);
                continue;
            };
            if c.centroids.is_empty() {
                cluster_ids.push(None);
                cluster_names.push(None);
                continue;
            }
            let mut best_id = 0usize;
            let mut best_sim = f32::NEG_INFINITY;
            for (i, centroid) in c.centroids.iter().enumerate() {
                let sim = dot(emb, centroid);
                if sim > best_sim {
                    best_sim = sim;
                    best_id = i;
                }
            }
            let threshold = c
                .assign_thresholds
                .get(best_id)
                .copied()
                .unwrap_or(f32::NEG_INFINITY);
            if best_sim < threshold {
                cluster_ids.push(None);
                cluster_names.push(None);
            } else {
                cluster_ids.push(Some(i32::try_from(best_id).unwrap_or(0)));
                cluster_names.push(c.names.get(best_id).cloned());
            }
        }
        df.with_column(Int32Chunked::new("topic_cluster_id".into(), cluster_ids).into_series())?;
        df.with_column(StringChunked::new("topic_cluster".into(), cluster_names).into_series())?;
    }

    // predicted_label + confidence
    if let Some(centroids) = label_centroids {
        let mut labels: Vec<Option<String>> = Vec::with_capacity(n);
        let mut scores: Vec<Option<f32>> = Vec::with_capacity(n);
        for emb_opt in embeddings {
            let Some(emb) = emb_opt else {
                labels.push(None);
                scores.push(None);
                continue;
            };
            if centroids.is_empty() {
                labels.push(None);
                scores.push(None);
                continue;
            }
            let mut best_label: Option<&String> = None;
            let mut best_sim = f32::NEG_INFINITY;
            for (label, centroid) in centroids {
                let sim = dot(emb, centroid);
                if sim > best_sim {
                    best_sim = sim;
                    best_label = Some(label);
                }
            }
            labels.push(best_label.cloned());
            scores.push(Some(best_sim));
        }
        df.with_column(StringChunked::new("predicted_label".into(), labels).into_series())?;
        df.with_column(Float32Chunked::new("confidence".into(), scores).into_series())?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lang_extracts_primary_subtag() {
        assert_eq!(normalize_lang("en-US"), Some("en".to_string()));
        assert_eq!(normalize_lang("PT_br"), Some("pt".to_string()));
        assert_eq!(normalize_lang("ja"), Some("ja".to_string()));
        assert_eq!(normalize_lang("  "), None);
        assert_eq!(normalize_lang(""), None);
    }

    #[test]
    fn language_histogram_sorted_desc() {
        let langs = vec![
            Some("en".to_string()),
            Some("en".to_string()),
            Some("ja".to_string()),
            None,
            Some("en".to_string()),
        ];
        let hist = build_language_histogram(&langs);
        assert_eq!(hist[0], ("en".to_string(), 3));
        assert_eq!(hist[1], ("ja".to_string(), 1));
    }

    #[test]
    fn language_gate_blanks_other_languages_keeps_untagged() {
        let facet = LanguageFacet {
            target: Some("en".to_string()),
            histogram: Vec::new(),
            row_langs: vec![Some("en".to_string()), Some("ja".to_string()), None],
        };
        let mut cleaned = vec![
            Some("hello world one".to_string()),
            Some("こんにちは世界です".to_string()),
            Some("untagged text row".to_string()),
        ];
        apply_language_gate(&mut cleaned, &facet);
        assert!(cleaned[0].is_some());
        assert!(cleaned[1].is_none(), "non-dominant language must be gated");
        assert!(cleaned[2].is_some(), "untagged rows are kept");
    }

    #[test]
    fn invalid_embeddings_rejected() {
        assert!(!is_valid_embedding(&[0.0, 0.0, 0.0]));
        assert!(!is_valid_embedding(&[f32::NAN, 1.0]));
        assert!(is_valid_embedding(&[0.1, 0.2]));
    }
}

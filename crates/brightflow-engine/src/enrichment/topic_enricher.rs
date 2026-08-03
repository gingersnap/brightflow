//! Topic enrichment: fit a topics model over a table's text, then apply it
//! to every synced row.
//!
//! # Why this is the way it is
//!
//! Topic clusters used to group issues by **surface format** ("backport
//! scripts", "stack traces") rather than by what a ticket is *about* ("auth
//! failure", "data loss"). That is structural to static token-averaged
//! embeddings, not a tuning bug: Model2Vec `potion-base-32M`'s dominant axis
//! of variance tracks format/vocabulary, so any *unsupervised* consumer of
//! that geometry — k-means, HDBSCAN, nearest-centroid — follows it. No choice
//! of `k`, cleaning profile, or distance metric fixes it, and better LLM
//! cluster naming cannot fix it either: the `auto_label` agent faithfully
//! names format clusters ("Automated Backport Commits" is an *accurate* label
//! for a cluster of backport commits). The clustering is upstream of the
//! problem.
//!
//! Supervision is the one mode that defeats the format axis: labels let a
//! trained head *downweight* format-correlated dimensions and *upweight*
//! content ones (nearest-centroid cannot — it weights every dimension equally
//! by construction). It is also the one task family where static embeddings
//! reach near-LLM quality (MTEB Classification), so a transformer is not
//! needed on the hot path. See [`crate::nlp::linear`] for the head.
//!
//! This is also why labels attach to **rows, not clusters**. The old
//! `refresh_label_artifact` built each label's centroid from
//! `clustering.centroids[i]` — the *cluster* centroid, not row embeddings —
//! so `predicted_label` was the cluster assignment wearing a nicer name, and
//! since clusters are format-shaped, cluster labels re-taught the format bias
//! by construction.
//!
//! # Cost invariant
//!
//! LLM cost is O(taxonomy + seed sample), never O(rows). The LLM proposes a
//! vocabulary a human ratifies (cold path, sampled); the hot path is
//! deterministic matrix-multiply classification that reaches every synced row
//! for free. [`enrich_with_topics`] never fits — it only applies artifacts.
//!
//! # Clusters are discovery, not the answer
//!
//! Clusters still earn their place, but not as labels. They surface themes the
//! taxonomy has not named (discovery), stratify the LLM seed sample across the
//! corpus (we want *diversity* from format clusters, not correctness), and
//! carry the low-confidence tail that feeds back into `propose_taxonomy`.
//! `predicted_labels` is the answer.
//!
//! # Sharp edges
//!
//! - `confidence` changed meaning: it was a raw cosine, now an uncalibrated
//!   sigmoid score. Neither is a probability — do not read 0.7 as "70% likely".
//! - Under the centroid fallback `predicted_labels` is null. A centroid model
//!   has no multi-label decision rule; faking one would make the column lie.
//!
//! # Graceful degradation
//!
//! Too little labelled signal → no trained head → the centroid fallback keeps
//! pre-classifier artifact directories working. `Labeler::Classifier` wins
//! when present; `Labeler::Centroids` is a compatibility path. A refit that
//! finds too little signal removes any stale head rather than scoring rows
//! against labels that no longer exist.
//!
//! # Verification
//!
//! The thesis is a test: `tests/classifier_quality.rs` plants intent labels
//! crossed orthogonally with format strata and asserts a **fair**
//! nearest-centroid baseline still loses (same split, same retained labels,
//! same threshold protocol). Its vacuity guard aborts if the baseline scores
//! too well — the corpus failed to reproduce the confound. On real data run
//! `topics fit` then `topics eval-classifier`; if the head does not win, the
//! thesis is wrong for that corpus and you stop.

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
    clean_for_embedding, default_min_cluster_size, effective_model_id, fit_multilabel_linear,
    hdbscan_dense, kmeans_dense, CleaningProfile, FittedTfIdf, TfIdf, Tokenizer, TokenizerPreset,
};

use super::artifacts::{
    ArtifactError, ArtifactMeta, ClassifierArtifact, ClusteringArtifact, LabelCentroidsArtifact,
    TfIdfArtifact, ARTIFACT_VERSION,
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
    /// Curated per-row intent labels, aligned with the DataFrame.
    ///
    /// The engine is storage-agnostic, so the API/CLI reads curated
    /// `document_labels` from SQLite and passes them here rather than the
    /// engine growing a database dependency. When absent, the fit falls back to
    /// the table's own `label_names` column so existing behaviour is preserved.
    pub labels: Option<LabelTargets>,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            num_clusters: DEFAULT_K,
            language: None,
            algorithm: None,
            labels: None,
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
    /// Held-out macro-F1 of the trained head, when one was trained. `None`
    /// means there was too little labelled signal — the fit degrades to the
    /// centroid fallback rather than failing.
    pub classifier_val_macro_f1: Option<f32>,
    /// Labels the trained head covers (after `MIN_LABEL_SUPPORT` filtering).
    pub classifier_labels: Vec<String>,
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
pub fn english_stopwords() -> std::collections::HashSet<String> {
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
pub type RowEmbeddings = Vec<Option<Vec<f32>>>;

/// Read an existing `embedding` column. Returns None if absent or wrong shape.
///
/// Public because near-duplicate detection and the API both need to read
/// embeddings back out of an enriched frame; this is the single definition.
pub fn read_existing_embeddings(df: &DataFrame, dim: usize) -> Option<RowEmbeddings> {
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
        "predicted_labels",
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

/// Per-row label assignments in a compact index space.
///
/// This is the supervision signal, decoupled from where it came from. The
/// engine is storage-agnostic, so curated row labels are **passed in** by the
/// API/CLI caller rather than read from SQLite here.
#[derive(Debug, Clone, Default)]
pub struct LabelTargets {
    /// Label names, indexed by the values in `per_row`.
    pub names: Vec<String>,
    /// Per-row label-index sets, aligned with the DataFrame. Empty = unlabelled.
    pub per_row: Vec<Vec<usize>>,
}

impl LabelTargets {
    /// Build from per-row label-name sets aligned with the DataFrame.
    pub fn from_row_labels(rows: &[Vec<String>]) -> Self {
        let mut names: Vec<String> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        // Two passes so `names` ends up in first-appearance order, which is
        // stable for a given input and therefore keeps fits reproducible.
        for row in rows {
            for label in row {
                let label = label.trim();
                if label.is_empty() {
                    continue;
                }
                if !index.contains_key(label) {
                    index.insert(label, names.len());
                    names.push(label.to_string());
                }
            }
        }
        let per_row = rows
            .iter()
            .map(|row| {
                let mut ids: Vec<usize> = row
                    .iter()
                    .filter_map(|l| index.get(l.trim()).copied())
                    .collect();
                ids.sort_unstable();
                ids.dedup();
                ids
            })
            .collect();
        Self { names, per_row }
    }

    /// Rows carrying at least one label.
    pub fn labelled_rows(&self) -> usize {
        self.per_row.iter().filter(|r| !r.is_empty()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Read comma-encoded labels from a string column into [`LabelTargets`].
///
/// Used for the legacy `label_names` path and by callers that keep labels in a
/// parquet column. Returns `None` when the column is absent or carries no
/// labels at all.
pub fn parse_label_targets(df: &DataFrame, column: &str) -> Option<LabelTargets> {
    let label_ca = df
        .column(column)
        .ok()
        .and_then(|c| c.as_materialized_series().str().ok().cloned())?;
    let rows: Vec<Vec<String>> = label_ca
        .into_iter()
        .map(|opt| {
            opt.map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
        })
        .collect();
    let targets = LabelTargets::from_row_labels(&rows);
    if targets.is_empty() {
        None
    } else {
        Some(targets)
    }
}

/// Build dense label centroids by averaging row embeddings per label.
///
/// Note this averages **row** embeddings, which is what makes it a legitimate
/// (if format-biased) baseline. The API's `refresh_label_artifact` writes the
/// same `labels.bin` from *cluster* centroids — a semantically different object
/// under one filename.
fn build_dense_label_centroids(
    targets: &LabelTargets,
    embeddings: &[Option<Vec<f32>>],
    dim: usize,
) -> HashMap<String, Vec<f32>> {
    let mut accum: HashMap<usize, (Vec<f32>, u32)> = HashMap::new();

    for (i, label_ids) in targets.per_row.iter().enumerate() {
        let Some(Some(emb)) = embeddings.get(i) else {
            continue;
        };
        if emb.len() != dim {
            continue;
        }
        for &id in label_ids {
            let entry = accum.entry(id).or_insert_with(|| (vec![0.0_f32; dim], 0));
            for (slot, val) in entry.0.iter_mut().zip(emb.iter()) {
                *slot += val;
            }
            entry.1 += 1;
        }
    }

    accum
        .into_iter()
        .filter_map(|(id, (mut sum, count))| {
            if count == 0 {
                return None;
            }
            let name = targets.names.get(id)?.clone();
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
            Some((name, sum))
        })
        .collect()
}

/// Train the supervised head over rows having both an embedding and >=1 label.
///
/// Returns `None` when there is too little signal to train — that is graceful
/// degradation, not an error: the caller falls back to label centroids.
fn train_classifier(
    targets: &LabelTargets,
    embeddings: &[Option<Vec<f32>>],
    dim: usize,
    model_id: &str,
) -> Option<ClassifierArtifact> {
    if targets.is_empty() {
        return None;
    }
    let mut features: Vec<Vec<f32>> = Vec::new();
    let mut row_targets: Vec<Vec<usize>> = Vec::new();
    for (i, label_ids) in targets.per_row.iter().enumerate() {
        if label_ids.is_empty() {
            continue;
        }
        let Some(Some(emb)) = embeddings.get(i) else {
            continue;
        };
        if emb.len() != dim {
            continue;
        }
        features.push(emb.clone());
        row_targets.push(label_ids.clone());
    }

    let outcome = fit_multilabel_linear(&features, &row_targets, targets.names.len(), dim)?;

    // Map the head's retained label indices back to names. The head's weight
    // rows are parallel to `retained_labels`, NOT to the caller's label space —
    // `MIN_LABEL_SUPPORT` drops under-supported labels, so these must be
    // re-aligned or every score would be attributed to the wrong label.
    let labels: Vec<String> = outcome
        .retained_labels
        .iter()
        .filter_map(|&i| targets.names.get(i).cloned())
        .collect();
    if labels.len() != outcome.head.weights.len() {
        return None;
    }

    Some(ClassifierArtifact {
        head: outcome.head,
        labels,
        dim,
        embedding_model_id: model_id.to_string(),
        fitted_at: Utc::now().timestamp(),
        artifact_version: ARTIFACT_VERSION,
        val_macro_f1: outcome.val_macro_f1,
        support: outcome.support,
        train_rows: outcome.train_rows,
    })
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
                Some((i, crate::nlp::similarity::dot_dense(v, centroid)))
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

    // 7. Resolve label supervision, then build dense label centroids.
    //    Curated row labels win; `label_names` is the backward-compatible
    //    fallback so tables without curation keep their current behaviour.
    let t_labels = std::time::Instant::now();
    let targets = options
        .labels
        .clone()
        .or_else(|| parse_label_targets(df, "label_names"))
        .unwrap_or_default();
    let label_centroids = build_dense_label_centroids(&targets, &embeddings, backend.dim());
    let has_labels = !label_centroids.is_empty();
    info!(
        "Built {} label centroids from {} labelled rows in {:?}",
        label_centroids.len(),
        targets.labelled_rows(),
        t_labels.elapsed()
    );

    // 7b. Train the supervised head. This is what turns format clusters into
    //     intent labels; centroids remain only as a fallback.
    let t_train = std::time::Instant::now();
    let classifier = train_classifier(&targets, &embeddings, backend.dim(), &model_id);
    if let Some(c) = &classifier {
        info!(
            "Trained classifier: {} labels, {} train rows, val_macro_f1={:.3} in {:?}",
            c.labels.len(),
            c.train_rows,
            c.val_macro_f1,
            t_train.elapsed()
        );
    } else {
        info!("No classifier trained (insufficient labelled signal); using centroids");
    }

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

    match &classifier {
        Some(c) => c.save(&artifact_dir)?,
        // Remove any previous head: a refit that finds too little signal must
        // not leave a stale head behind scoring rows against labels that no
        // longer exist.
        None => ClassifierArtifact::remove(&artifact_dir)?,
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
        has_classifier: classifier.is_some(),
        classifier_val_macro_f1: classifier.as_ref().map(|c| c.val_macro_f1),
    };
    meta.save(&artifact_dir)?;
    info!("Saved artifacts to {}", artifact_dir.display());

    // 9. Build enriched DataFrame.
    let t_enrich = std::time::Instant::now();
    let mut work = df.clone();
    drop_enrichment_columns(&mut work);
    write_embedding_columns(&mut work, &embeddings, &model_id, backend.dim())?;

    // Classifier wins when present; centroids are the fallback.
    let labeler = classifier
        .as_ref()
        .map(Labeler::Classifier)
        .or_else(|| has_labels.then_some(Labeler::Centroids(&label_centroids)));
    apply_topics(
        &mut work,
        &embeddings,
        &tfidf_vectors,
        &fitted_tfidf,
        Some(&clustering),
        labeler.as_ref(),
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
            classifier_val_macro_f1: classifier.as_ref().map(|c| c.val_macro_f1),
            classifier_labels: classifier.map(|c| c.labels).unwrap_or_default(),
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

    // Prefer the trained head; fall back to label centroids. A missing or
    // incompatible `classifier.bin` degrades to the old behaviour rather than
    // failing — this is what keeps pre-classifier artifact dirs working.
    let classifier = ClassifierArtifact::load(&artifact_dir)
        .ok()
        .filter(|a| a.is_compatible(&model_id, backend.dim()));
    let centroids = LabelCentroidsArtifact::load(&artifact_dir)
        .ok()
        .filter(|a| a.embedding_model_id == model_id)
        .map(|a| a.centroids);

    let mut work = df.clone();
    drop_enrichment_columns(&mut work);
    write_embedding_columns(&mut work, &embeddings, &model_id, backend.dim())?;

    if let (Some(t_artifact), Some(t_vectors)) = (tfidf_artifact.as_ref(), tfidf_vectors.as_ref()) {
        let labeler = classifier
            .as_ref()
            .map(Labeler::Classifier)
            .or_else(|| centroids.as_ref().map(Labeler::Centroids));
        apply_topics(
            &mut work,
            &embeddings,
            t_vectors,
            &t_artifact.fitted,
            clustering.as_ref(),
            labeler.as_ref(),
        )?;
    }

    Ok(work)
}

/// How rows get labelled.
///
/// `Classifier` is the answer; `Centroids` is a compatibility fallback for
/// artifact directories fitted before the head existed. The distinction is not
/// cosmetic — a centroid model has no multi-label decision rule, so it cannot
/// honestly populate `predicted_labels`.
#[derive(Debug, Clone, Copy)]
pub enum Labeler<'a> {
    Classifier(&'a ClassifierArtifact),
    Centroids(&'a HashMap<String, Vec<f32>>),
}

/// Apply enrichment columns: topic_terms, topic_cluster*, predicted_label,
/// predicted_labels, confidence.
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
    labeler: Option<&Labeler<'_>>,
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
                let sim = crate::nlp::similarity::dot_dense(emb, centroid);
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
        // KNOWN DRIFT: `topic_cluster` snapshots the cluster NAME at fit time,
        // but no reader consumes it — the topics API resolves names from
        // `clusters.bin` through the curation overlay, keyed on
        // `topic_cluster_id`. So a rename or a merge updates what the UI shows
        // while this column keeps the old name until the next refit. It is kept
        // only for external parquet consumers; treat `topic_cluster_id` as the
        // identity and never join on this string.
        df.with_column(StringChunked::new("topic_cluster".into(), cluster_names).into_series())?;
    }

    // predicted_label + confidence (+ predicted_labels under a trained head)
    if let Some(labeler) = labeler {
        write_label_columns(df, embeddings, *labeler)?;
    }

    Ok(())
}

/// Write `predicted_label`, `confidence` and (head only) `predicted_labels`.
///
/// `Labeler` is `Copy` (it holds only references), so it is taken by value.
fn write_label_columns(
    df: &mut DataFrame,
    embeddings: &[Option<Vec<f32>>],
    labeler: Labeler<'_>,
) -> Result<(), TopicError> {
    let n = embeddings.len();
    let mut labels: Vec<Option<String>> = Vec::with_capacity(n);
    let mut scores: Vec<Option<f32>> = Vec::with_capacity(n);
    let mut multi: Vec<Option<String>> = Vec::with_capacity(n);

    for emb_opt in embeddings {
        let Some(emb) = emb_opt else {
            labels.push(None);
            scores.push(None);
            multi.push(None);
            continue;
        };
        match labeler {
            Labeler::Classifier(artifact) => {
                let best = artifact
                    .head
                    .argmax(emb)
                    .and_then(|(i, s)| artifact.labels.get(i).map(|l| (l.clone(), s)));
                if let Some((label, score)) = best {
                    labels.push(Some(label));
                    scores.push(Some(score));
                } else {
                    labels.push(None);
                    scores.push(None);
                }
                let hits: Vec<&str> = artifact
                    .head
                    .predict(emb)
                    .into_iter()
                    .filter_map(|(i, _)| artifact.labels.get(i).map(String::as_str))
                    .collect();
                // Empty means "no intent confidently applies" — encode that as
                // null, not "", so consumers can tell it apart from a label.
                multi.push(if hits.is_empty() {
                    None
                } else {
                    Some(hits.join(", "))
                });
            },
            Labeler::Centroids(centroids) => {
                if centroids.is_empty() {
                    labels.push(None);
                    scores.push(None);
                } else {
                    let mut best_label: Option<&String> = None;
                    let mut best_sim = f32::NEG_INFINITY;
                    for (label, centroid) in centroids {
                        if centroid.len() != emb.len() {
                            continue;
                        }
                        let sim = crate::nlp::similarity::dot_dense(emb, centroid);
                        if sim > best_sim {
                            best_sim = sim;
                            best_label = Some(label);
                        }
                    }
                    labels.push(best_label.cloned());
                    scores.push(best_label.map(|_| best_sim));
                }
                // A centroid model has no multi-label decision rule. Faking one
                // (e.g. "every centroid above some cosine") would make the
                // column lie about what the model actually decided.
                multi.push(None);
            },
        }
    }

    df.with_column(StringChunked::new("predicted_label".into(), labels).into_series())?;
    df.with_column(Float32Chunked::new("confidence".into(), scores).into_series())?;
    df.with_column(StringChunked::new("predicted_labels".into(), multi).into_series())?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::cast_possible_wrap)]
mod tests {
    use super::*;
    use crate::nlp::MultiLabelLinear;

    fn test_head() -> ClassifierArtifact {
        ClassifierArtifact {
            head: MultiLabelLinear {
                // Label 0 fires on dim 0, label 1 on dim 1.
                weights: vec![vec![10.0, 0.0], vec![0.0, 10.0]],
                biases: vec![-1.0, -1.0],
                thresholds: vec![0.5, 0.5],
            },
            labels: vec!["auth failure".to_string(), "data loss".to_string()],
            dim: 2,
            embedding_model_id: "test-model".to_string(),
            fitted_at: 0,
            artifact_version: ARTIFACT_VERSION,
            val_macro_f1: 0.9,
            support: vec![50, 50],
            train_rows: 100,
        }
    }

    fn frame(n: usize) -> DataFrame {
        DataFrame::new(vec![Column::new(
            "id".into(),
            (0..n as i32).collect::<Vec<_>>(),
        )])
        .expect("frame")
    }

    #[test]
    fn label_targets_index_labels_in_first_appearance_order() {
        let rows = vec![
            vec!["beta".to_string(), "alpha".to_string()],
            vec!["alpha".to_string()],
            vec![],
        ];
        let t = LabelTargets::from_row_labels(&rows);
        assert_eq!(t.names, vec!["beta".to_string(), "alpha".to_string()]);
        assert_eq!(t.per_row, vec![vec![0, 1], vec![1], vec![]]);
        assert_eq!(t.labelled_rows(), 2);
    }

    #[test]
    fn label_targets_dedupe_and_trim() {
        let rows = vec![vec![" a ".to_string(), "a".to_string(), String::new()]];
        let t = LabelTargets::from_row_labels(&rows);
        assert_eq!(t.names, vec!["a".to_string()]);
        assert_eq!(
            t.per_row,
            vec![vec![0]],
            "a repeated label must not double-count"
        );
    }

    #[test]
    fn parse_label_targets_reads_comma_encoding() {
        let df = DataFrame::new(vec![Column::new(
            "label_names".into(),
            vec![Some("bug, perf"), Some("bug"), None],
        )])
        .expect("frame");
        let t = parse_label_targets(&df, "label_names").expect("labels");
        assert_eq!(t.names, vec!["bug".to_string(), "perf".to_string()]);
        assert_eq!(t.per_row, vec![vec![0, 1], vec![0], vec![]]);
    }

    #[test]
    fn parse_label_targets_none_when_column_absent_or_empty() {
        let df = frame(3);
        assert!(parse_label_targets(&df, "label_names").is_none());

        let empty = DataFrame::new(vec![Column::new(
            "label_names".into(),
            vec![None::<&str>, None, None],
        )])
        .expect("frame");
        assert!(parse_label_targets(&empty, "label_names").is_none());
    }

    #[test]
    fn classifier_labeler_writes_all_three_columns() {
        let artifact = test_head();
        let mut df = frame(3);
        let embeddings = vec![
            Some(vec![1.0, 0.0]), // label 0 only
            Some(vec![1.0, 1.0]), // both labels
            None,                 // ineligible row
        ];
        write_label_columns(&mut df, &embeddings, Labeler::Classifier(&artifact)).expect("write");

        let predicted = df.column("predicted_label").unwrap().str().unwrap();
        assert_eq!(predicted.get(0), Some("auth failure"));
        assert_eq!(predicted.get(2), None, "no embedding -> null label");

        let multi = df.column("predicted_labels").unwrap().str().unwrap();
        assert_eq!(multi.get(0), Some("auth failure"));
        let both = multi.get(1).expect("row 1 clears both thresholds");
        assert!(
            both.contains("auth failure") && both.contains("data loss"),
            "got {both}"
        );
        assert_eq!(multi.get(2), None, "no embedding -> null");

        let conf = df.column("confidence").unwrap().f32().unwrap();
        assert!(conf.get(0).unwrap_or(0.0) > 0.5);
        assert_eq!(conf.get(2), None);
    }

    #[test]
    fn classifier_labeler_writes_null_when_nothing_clears_threshold() {
        let artifact = test_head();
        let mut df = frame(1);
        // Origin scores sigmoid(-1) ~= 0.27 for both labels: under threshold.
        let embeddings = vec![Some(vec![0.0, 0.0])];
        write_label_columns(&mut df, &embeddings, Labeler::Classifier(&artifact)).expect("write");

        let multi = df.column("predicted_labels").unwrap().str().unwrap();
        assert_eq!(
            multi.get(0),
            None,
            "no label over threshold must be null, never an empty string"
        );
        // predicted_label is an unconditional argmax, so it still reports.
        assert!(df
            .column("predicted_label")
            .unwrap()
            .str()
            .unwrap()
            .get(0)
            .is_some());
    }

    #[test]
    fn centroid_labeler_leaves_predicted_labels_null() {
        // A centroid model has no multi-label decision rule. Populating
        // predicted_labels under the fallback would make the column lie.
        let mut centroids: HashMap<String, Vec<f32>> = HashMap::new();
        centroids.insert("auth failure".to_string(), vec![1.0, 0.0]);
        centroids.insert("data loss".to_string(), vec![0.0, 1.0]);

        let mut df = frame(2);
        let embeddings = vec![Some(vec![1.0, 0.0]), Some(vec![0.0, 1.0])];
        write_label_columns(&mut df, &embeddings, Labeler::Centroids(&centroids)).expect("write");

        let predicted = df.column("predicted_label").unwrap().str().unwrap();
        assert_eq!(predicted.get(0), Some("auth failure"));
        assert_eq!(predicted.get(1), Some("data loss"));

        let multi = df.column("predicted_labels").unwrap().str().unwrap();
        assert_eq!(multi.get(0), None);
        assert_eq!(multi.get(1), None);
    }

    #[test]
    fn drop_enrichment_columns_removes_predicted_labels() {
        // Without this, a refit leaves a stale predicted_labels column behind.
        let mut df = DataFrame::new(vec![
            Column::new("id".into(), vec![1]),
            Column::new("predicted_labels".into(), vec![Some("stale")]),
            Column::new("predicted_label".into(), vec![Some("stale")]),
        ])
        .expect("frame");
        drop_enrichment_columns(&mut df);
        assert!(df.column("predicted_labels").is_err(), "must be dropped");
        assert!(df.column("predicted_label").is_err());
        assert!(
            df.column("id").is_ok(),
            "non-enrichment columns must survive"
        );
    }

    #[test]
    fn build_dense_label_centroids_averages_row_embeddings() {
        let targets = LabelTargets::from_row_labels(&[
            vec!["a".to_string()],
            vec!["a".to_string()],
            vec!["b".to_string()],
        ]);
        let embeddings = vec![
            Some(vec![1.0, 0.0]),
            Some(vec![0.0, 1.0]),
            Some(vec![0.0, 1.0]),
        ];
        let centroids = build_dense_label_centroids(&targets, &embeddings, 2);
        assert_eq!(centroids.len(), 2);
        // "a" averages (1,0) and (0,1) -> (0.5,0.5) -> normalized ~ (0.707,0.707)
        let a = centroids.get("a").expect("a");
        assert!((a[0] - 0.707).abs() < 0.01, "got {a:?}");
        assert!((a[1] - 0.707).abs() < 0.01, "got {a:?}");
    }

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

use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use model2vec_rs::model::StaticModel;
use polars::prelude::*;
use thiserror::Error;
use tracing::info;

use crate::embedding::{
    embed_batch, sanitize_source_id, shared_embedder, topics_artifact_dir, EmbedderError,
    EMBEDDING_DIM, POTION_BASE_32M_DIR,
};
use crate::nlp::{kmeans_dense, FittedTfIdf, TfIdf, Tokenizer, TokenizerPreset};

use super::artifacts::{
    ArtifactError, ArtifactMeta, ClusteringArtifact, LabelCentroidsArtifact, TfIdfArtifact,
};

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

/// Output of a fit run.
#[derive(Debug, Clone)]
pub struct FitOutcome {
    pub k: usize,
    pub cluster_names: Vec<String>,
    pub cluster_sizes: Vec<usize>,
    pub total_rows: usize,
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

/// Combine `title` and `body` into a single text string for each row.
fn build_combined_text(df: &DataFrame) -> Result<Vec<String>, TopicError> {
    let title = df
        .column("title")
        .map_err(|_| TopicError::MissingColumn("title".to_string()))?
        .as_materialized_series()
        .str()
        .map_err(|e| TopicError::Nlp(format!("title is not a string column: {e}")))?
        .into_iter()
        .map(|o| o.unwrap_or("").to_string())
        .collect::<Vec<_>>();
    let body = df
        .column("body")
        .map_err(|_| TopicError::MissingColumn("body".to_string()))?
        .as_materialized_series()
        .str()
        .map_err(|e| TopicError::Nlp(format!("body is not a string column: {e}")))?
        .into_iter()
        .map(|o| o.unwrap_or("").to_string())
        .collect::<Vec<_>>();

    Ok(title
        .into_iter()
        .zip(body)
        .map(|(t, b)| format!("{t} {b}"))
        .collect())
}

/// Read an existing `embedding` column. Returns None if absent or wrong shape.
fn read_existing_embeddings(df: &DataFrame) -> Option<Vec<Option<Vec<f32>>>> {
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
                if v.len() != EMBEDDING_DIM {
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

/// Compute embeddings for all rows. Reuses any existing embeddings whose
/// `embedding_model_id` matches the current model.
fn compute_embeddings(
    df: &DataFrame,
    encoder: &StaticModel,
    model_id: &str,
) -> Result<(Vec<Vec<f32>>, usize), TopicError> {
    let texts = build_combined_text(df)?;
    let n = texts.len();

    let existing = read_existing_embeddings(df);
    let existing_ids = read_existing_model_ids(df);

    // Determine which rows need re-embedding.
    let mut to_embed_indices: Vec<usize> = Vec::new();
    let mut out: Vec<Option<Vec<f32>>> = vec![None; n];

    if let Some(ex) = existing {
        for (i, vec_opt) in ex.into_iter().enumerate() {
            let id_matches = existing_ids
                .get(i)
                .and_then(|o| o.as_deref())
                .is_some_and(|id| id == model_id);
            if id_matches {
                if let Some(v) = vec_opt {
                    out[i] = Some(v);
                    continue;
                }
            }
            to_embed_indices.push(i);
        }
    } else {
        to_embed_indices = (0..n).collect();
    }

    let embedded_count = to_embed_indices.len();
    if !to_embed_indices.is_empty() {
        let to_embed: Vec<String> = to_embed_indices.iter().map(|&i| texts[i].clone()).collect();
        let new_embs = embed_batch(encoder, &to_embed)?;
        for (idx_in_batch, &row_idx) in to_embed_indices.iter().enumerate() {
            if let Some(emb) = new_embs.get(idx_in_batch) {
                out[row_idx] = Some(emb.clone());
            }
        }
    }

    let final_vecs: Vec<Vec<f32>> = out
        .into_iter()
        .map(|o| o.unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]))
        .collect();

    Ok((final_vecs, embedded_count))
}

/// Build a `List<Float32>` series from per-row embeddings.
fn embeddings_to_series(name: &str, embeddings: &[Vec<f32>]) -> Series {
    let mut builder = ListPrimitiveChunkedBuilder::<Float32Type>::new(
        name.into(),
        embeddings.len(),
        embeddings.len() * EMBEDDING_DIM,
        DataType::Float32,
    );
    for emb in embeddings {
        builder.append_slice(emb);
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
    embeddings: &[Vec<f32>],
    model_id: &str,
) -> Result<(), TopicError> {
    let emb_series = embeddings_to_series("embedding", embeddings);
    let id_series = StringChunked::new(
        "embedding_model_id".into(),
        vec![Some(model_id); df.height()],
    )
    .into_series();
    df.with_column(emb_series)?;
    df.with_column(id_series)?;
    Ok(())
}

/// Build dense label centroids by averaging row embeddings per label.
fn build_dense_label_centroids(
    df: &DataFrame,
    embeddings: &[Vec<f32>],
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
        let Some(emb) = embeddings.get(i) else {
            continue;
        };
        for label in labels.split(',') {
            let label = label.trim();
            if label.is_empty() {
                continue;
            }
            let entry = accum
                .entry(label.to_string())
                .or_insert_with(|| (vec![0.0_f32; EMBEDDING_DIM], 0));
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
    cluster_result: &crate::nlp::DenseClusterResult,
    embeddings: &[Vec<f32>],
    titles: Option<&[Option<String>]>,
    n: usize,
) -> Vec<String> {
    let Some(titles) = titles else {
        return Vec::new();
    };
    let Some(centroid) = cluster_result.centroids.get(cluster_idx) else {
        return Vec::new();
    };
    if centroid.iter().all(|&v| v == 0.0) {
        return Vec::new();
    }

    let mut candidates: Vec<(usize, f32)> = cluster_result
        .assignments
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            if c == cluster_idx {
                let v = embeddings.get(i)?;
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
    cluster_result: &crate::nlp::DenseClusterResult,
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

    for (row_idx, &cluster) in cluster_result.assignments.iter().enumerate() {
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

/// Fit the full pipeline: TF-IDF + dense k-means + label centroids.
/// Saves artifacts to disk and returns an enriched DataFrame.
pub fn fit_topics(
    workspace_root: &Path,
    source_id: &str,
    table_name: &str,
    df: &DataFrame,
    num_clusters: usize,
) -> Result<(DataFrame, FitOutcome), TopicError> {
    let encoder = shared_embedder(workspace_root)?;
    let model_id = POTION_BASE_32M_DIR.to_string();
    let artifact_dir = topics_artifact_dir(workspace_root, source_id, table_name);

    info!(
        "Fitting topics for source={} table={} (sanitized={}) k={}",
        source_id,
        table_name,
        sanitize_source_id(source_id),
        num_clusters
    );

    // 1. Embed (reuse existing where model_id matches).
    let (embeddings, embedded_count) = compute_embeddings(df, &encoder, &model_id)?;
    info!(
        "Computed {} embeddings ({} reused, {} fresh)",
        embeddings.len(),
        embeddings.len() - embedded_count,
        embedded_count
    );

    let texts = build_combined_text(df)?;

    // 2. Fit TF-IDF (for top-term naming).
    let t_fit = std::time::Instant::now();
    let stopwords = english_stopwords();
    info!("Loaded {} stopwords for TF-IDF", stopwords.len());
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
        .fit(&texts)
        .map_err(|e| TopicError::Nlp(e.to_string()))?;
    info!(
        "Fitted TF-IDF: {} vocab terms in {:?}",
        fitted_tfidf.vocabulary.len(),
        t_fit.elapsed()
    );
    let t_transform = std::time::Instant::now();
    let tfidf_vectors = fitted_tfidf.transform_batch(&texts);
    info!("TF-IDF transform_batch in {:?}", t_transform.elapsed());

    // 3. Cluster in embedding space.
    let k = num_clusters.max(1).min(embeddings.len().max(1));
    let t_kmeans = std::time::Instant::now();
    let cluster_result = kmeans_dense(&embeddings, k, KMEANS_MAX_ITER);
    info!(
        "k-means k={k} converged in {} iterations, {:?}",
        cluster_result.iterations,
        t_kmeans.elapsed()
    );

    // 4. Cluster sizes from kmeans.
    let mut cluster_sizes: Vec<usize> = vec![0; k];
    for &assignment in &cluster_result.assignments {
        if assignment < k {
            cluster_sizes[assignment] += 1;
        }
    }
    info!("Cluster sizes (kmeans): {:?}", cluster_sizes);

    // 5. Per-cluster naming:
    //    - sample_titles: 3 representative doc titles (closest to centroid)
    //    - top_terms: c-TF-IDF distinctive terms (distinguish this cluster
    //      from the others, not just frequent within it)
    //    - names: a short single-line name combining the most-central title
    //      with the top distinctive terms. Used in CLI/legacy summaries.
    let titles = read_string_column(df, "title");
    let mut sample_titles: Vec<Vec<String>> = Vec::with_capacity(k);
    for cluster_idx in 0..k {
        sample_titles.push(representative_titles(
            cluster_idx,
            &cluster_result,
            &embeddings,
            titles.as_deref(),
            SAMPLE_TITLES_PER_CLUSTER,
        ));
    }
    let top_terms = build_ctfidf_top_terms(
        k,
        &cluster_result,
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

    // 5. Build dense label centroids.
    let t_labels = std::time::Instant::now();
    let label_centroids = build_dense_label_centroids(df, &embeddings);
    let has_labels = !label_centroids.is_empty();
    info!(
        "Built {} label centroids in {:?}",
        label_centroids.len(),
        t_labels.elapsed()
    );

    // 6. Persist artifacts.
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
    };
    meta.save(&artifact_dir)?;
    info!("Saved artifacts to {}", artifact_dir.display());

    // 7. Build enriched DataFrame.
    let t_enrich = std::time::Instant::now();
    let mut work = df.clone();
    drop_enrichment_columns(&mut work);
    write_embedding_columns(&mut work, &embeddings, &model_id)?;

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
            has_labels,
        },
    ))
}

/// Embed-and-assign-only: never re-fits.
///
/// Applies whatever artifacts exist on disk to the DataFrame. Always writes
/// `embedding` and `embedding_model_id`; topic columns are written only when
/// fitted artifacts are present.
pub fn enrich_with_topics(
    workspace_root: &Path,
    source_id: &str,
    table_name: &str,
    df: &DataFrame,
) -> Result<DataFrame, TopicError> {
    let encoder = shared_embedder(workspace_root)?;
    let model_id = POTION_BASE_32M_DIR.to_string();
    let artifact_dir = topics_artifact_dir(workspace_root, source_id, table_name);

    let (embeddings, embedded_count) = compute_embeddings(df, &encoder, &model_id)?;
    info!(
        "Enriched {} rows ({} fresh embeddings) for {table_name}",
        embeddings.len(),
        embedded_count
    );

    // Recompute TF-IDF vectors per row only if a fitted TF-IDF artifact exists.
    let tfidf_artifact = TfIdfArtifact::load(&artifact_dir).ok();
    let texts = build_combined_text(df)?;
    let tfidf_vectors = tfidf_artifact
        .as_ref()
        .map(|a| a.fitted.transform_batch(&texts));

    let clustering = ClusteringArtifact::load(&artifact_dir)
        .ok()
        .filter(|a| a.embedding_model_id == model_id);
    let labels = LabelCentroidsArtifact::load(&artifact_dir)
        .ok()
        .filter(|a| a.embedding_model_id == model_id)
        .map(|a| a.centroids);

    let mut work = df.clone();
    drop_enrichment_columns(&mut work);
    write_embedding_columns(&mut work, &embeddings, &model_id)?;

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
fn apply_topics(
    df: &mut DataFrame,
    embeddings: &[Vec<f32>],
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
        for emb in embeddings {
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
            cluster_ids.push(Some(i32::try_from(best_id).unwrap_or(0)));
            cluster_names.push(c.names.get(best_id).cloned());
        }
        df.with_column(Int32Chunked::new("topic_cluster_id".into(), cluster_ids).into_series())?;
        df.with_column(StringChunked::new("topic_cluster".into(), cluster_names).into_series())?;
    }

    // predicted_label + confidence
    if let Some(centroids) = label_centroids {
        let mut labels: Vec<Option<String>> = Vec::with_capacity(n);
        let mut scores: Vec<Option<f32>> = Vec::with_capacity(n);
        for emb in embeddings {
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

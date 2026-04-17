use std::collections::HashMap;
use std::path::Path;

use polars::prelude::*;

use crate::nlp::clustering::kmeans;
use crate::nlp::error::{Result, SubtextError};
use crate::nlp::{FittedTfIdf, SparseVec, TfIdf, TokenizerPreset};

use super::centroids::build_label_centroids;
use super::serde_utils::sparse_vec_to_bytes;
use super::transform::nearest_label;

/// A persisted text enrichment model: TF-IDF + label centroids + cluster centroids.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TextEnrichmentModel {
    /// The fitted TF-IDF model.
    pub fitted: FittedTfIdf,
    /// Label centroids (from labeled data).
    pub label_centroids: HashMap<String, SparseVec>,
    /// Cluster centroids (from k-means).
    pub cluster_centroids: Vec<SparseVec>,
    /// Human-readable cluster names (top terms per cluster).
    pub cluster_names: Vec<String>,
}

/// Configuration for text enrichment.
pub struct EnrichmentConfig {
    /// Column containing text to analyze.
    pub text_col: String,
    /// Column containing comma-separated labels (for label-derived topics).
    /// If None, label prediction is skipped.
    pub label_col: Option<String>,
    /// Number of topic clusters for corpus-derived topics.
    pub num_clusters: usize,
    /// Number of top terms to include in topic_terms column.
    pub top_n_terms: usize,
    /// Number of top terms used to name clusters.
    pub cluster_name_terms: usize,
    /// TF-IDF n-gram range.
    pub ngram_range: std::ops::RangeInclusive<usize>,
    /// Minimum document frequency for vocabulary filtering.
    pub min_df: f32,
    /// Tokenizer preset.
    pub preset: TokenizerPreset,
    /// Maximum k-means iterations.
    pub max_kmeans_iter: usize,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            text_col: "text".to_string(),
            label_col: None,
            num_clusters: 8,
            top_n_terms: 5,
            cluster_name_terms: 3,
            ngram_range: 1..=1,
            min_df: 2.0,
            preset: TokenizerPreset::CodeAware,
            max_kmeans_iter: 20,
        }
    }
}

impl TextEnrichmentModel {
    /// Save the model to a file (bincode format).
    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes =
            bincode::serialize(self).map_err(|e| SubtextError::Serialization(e.to_string()))?;
        std::fs::write(path, bytes)
            .map_err(|e| SubtextError::Other(format!("failed to write model: {e}")))?;
        Ok(())
    }

    /// Load a model from a file (bincode format).
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .map_err(|e| SubtextError::Other(format!("failed to read model: {e}")))?;
        bincode::deserialize(&bytes).map_err(|e| SubtextError::Serialization(e.to_string()))
    }
}

/// Fit a text enrichment model from a DataFrame.
///
/// Returns the model AND the pre-computed vectors (so callers can reuse them
/// for enrichment without re-transforming).
pub fn fit_enrichment_model(
    df: &DataFrame,
    config: &EnrichmentConfig,
) -> Result<(TextEnrichmentModel, Vec<SparseVec>)> {
    let text_series = df
        .column(&config.text_col)
        .map_err(|_| SubtextError::ColumnNotFound(config.text_col.clone()))?
        .as_materialized_series();

    let ca = text_series.str().map_err(|_| SubtextError::TypeMismatch {
        expected: "String".to_string(),
        actual: format!("{:?}", text_series.dtype()),
    })?;
    let docs: Vec<&str> = ca.into_no_null_iter().collect();

    let fitted = TfIdf::new()
        .preset(config.preset)
        .ngram_range(config.ngram_range.clone())
        .min_df(config.min_df)
        .sublinear_tf(true)
        .fit(&docs)?;

    // Build label centroids if label column is provided
    let label_centroids = if let Some(label_col) = &config.label_col {
        build_label_centroids(df, &config.text_col, label_col, &fitted)?
    } else {
        HashMap::new()
    };

    // Transform all docs once — reused for enrichment
    let vectors = fitted.transform_batch(&docs);

    // Run k-means clustering if requested
    let (cluster_centroids, cluster_names) = if config.num_clusters > 0 {
        let num_clusters = config.num_clusters.min(vectors.len().max(1));
        let cluster_result = kmeans(&vectors, num_clusters, config.max_kmeans_iter);

        let names: Vec<String> = cluster_result
            .centroids
            .iter()
            .map(|centroid| {
                let terms = fitted.top_term_strings(centroid, config.cluster_name_terms);
                if terms.is_empty() {
                    "other".to_string()
                } else {
                    terms.join(", ")
                }
            })
            .collect();

        (cluster_result.centroids, names)
    } else {
        (Vec::new(), Vec::new())
    };

    let model = TextEnrichmentModel {
        fitted,
        label_centroids,
        cluster_centroids,
        cluster_names,
    };

    Ok((model, vectors))
}

/// Build enrichment columns from pre-computed vectors (no re-transformation).
///
/// This is the fast path used when vectors are already available from fitting.
#[allow(clippy::too_many_lines)]
fn build_enrichment_columns(
    df: &DataFrame,
    vectors: &[SparseVec],
    model: &TextEnrichmentModel,
    top_n_terms: usize,
) -> Result<DataFrame> {
    let n = vectors.len();
    let mut topic_terms_col: Vec<Option<String>> = Vec::with_capacity(n);
    let mut topic_cluster_col: Vec<Option<String>> = Vec::with_capacity(n);
    let mut tfidf_binary_col: Vec<Option<Vec<u8>>> = Vec::with_capacity(n);

    for vec in vectors {
        if vec.is_empty() {
            topic_terms_col.push(None);
            topic_cluster_col.push(None);
            tfidf_binary_col.push(None);
            continue;
        }

        // Top terms
        let terms = model.fitted.top_term_strings(vec, top_n_terms);
        topic_terms_col.push(Some(terms.join(", ")));

        // Cluster assignment
        if model.cluster_centroids.is_empty() {
            topic_cluster_col.push(None);
        } else {
            let mut best_cluster = 0;
            let mut best_sim = f32::NEG_INFINITY;
            for (i, centroid) in model.cluster_centroids.iter().enumerate() {
                let sim = crate::nlp::cosine(vec, centroid);
                if sim > best_sim {
                    best_sim = sim;
                    best_cluster = i;
                }
            }
            let name = model
                .cluster_names
                .get(best_cluster)
                .cloned()
                .unwrap_or_else(|| format!("cluster_{best_cluster}"));
            topic_cluster_col.push(Some(name));
        }

        tfidf_binary_col.push(sparse_vec_to_bytes(vec).ok());
    }

    let mut result = df.clone();

    let terms_series = StringChunked::new("topic_terms".into(), topic_terms_col).into_series();
    result.with_column(terms_series)?;

    let cluster_series =
        StringChunked::new("topic_cluster".into(), topic_cluster_col).into_series();
    result.with_column(cluster_series)?;

    if !model.label_centroids.is_empty() {
        let binary_series = BinaryChunked::new(
            "_tfidf_vec".into(),
            tfidf_binary_col
                .iter()
                .map(|opt| opt.as_deref())
                .collect::<Vec<_>>(),
        )
        .into_series();

        let (label_series, score_series) = nearest_label(&binary_series, &model.label_centroids)?;
        result.with_column(label_series)?;
        result.with_column(score_series)?;
    }

    drop(result.drop_in_place("_tfidf_vec"));
    Ok(result)
}

/// Enrich a DataFrame with text-derived columns using a fitted model.
///
/// This re-transforms each document. Use `build_enrichment_columns` with
/// pre-computed vectors when available for better performance.
pub fn enrich_dataframe(
    df: &DataFrame,
    text_col: &str,
    model: &TextEnrichmentModel,
    top_n_terms: usize,
) -> Result<DataFrame> {
    let text_series = df
        .column(text_col)
        .map_err(|_| SubtextError::ColumnNotFound(text_col.to_string()))?
        .as_materialized_series();

    let ca = text_series.str().map_err(|_| SubtextError::TypeMismatch {
        expected: "String".to_string(),
        actual: format!("{:?}", text_series.dtype()),
    })?;
    let docs: Vec<&str> = ca.into_no_null_iter().collect();
    let vectors = model.fitted.transform_batch(&docs);

    build_enrichment_columns(df, &vectors, model, top_n_terms)
}

/// Convenience: fit a model and enrich a GitHub issues DataFrame in one step.
///
/// Expects columns: `title`, `body`, `label_names`.
/// Transforms documents only once (shared between fitting and enrichment).
pub fn enrich_github_issues(
    df: &DataFrame,
    model_path: Option<&Path>,
    num_clusters: usize,
) -> Result<(DataFrame, TextEnrichmentModel)> {
    // Combine title + body
    let title = df
        .column("title")
        .map_err(|_| SubtextError::ColumnNotFound("title".to_string()))?
        .as_materialized_series()
        .str()
        .map_err(|_| SubtextError::TypeMismatch {
            expected: "String".to_string(),
            actual: "non-string".to_string(),
        })?;
    let body = df
        .column("body")
        .map_err(|_| SubtextError::ColumnNotFound("body".to_string()))?
        .as_materialized_series()
        .str()
        .map_err(|_| SubtextError::TypeMismatch {
            expected: "String".to_string(),
            actual: "non-string".to_string(),
        })?;

    let combined: StringChunked = title
        .into_iter()
        .zip(body)
        .map(|(t, b)| {
            let title_str = t.unwrap_or("");
            let body_str = b.unwrap_or("");
            Some(format!("{title_str} {body_str}"))
        })
        .collect();

    let mut work_df = df.clone();
    work_df.with_column(combined.with_name("_combined_text".into()).into_series())?;

    // Load existing model or fit new one
    let (model, vectors) = if let Some(path) = model_path {
        if path.exists() {
            let model = TextEnrichmentModel::load(path)?;
            // Re-transform with loaded model (no k-means needed)
            let ca = work_df
                .column("_combined_text")?
                .as_materialized_series()
                .str()?;
            let docs: Vec<&str> = ca.into_no_null_iter().collect();
            let vectors = model.fitted.transform_batch(&docs);
            (model, vectors)
        } else {
            fit_new_model(&work_df, num_clusters)?
        }
    } else {
        fit_new_model(&work_df, num_clusters)?
    };

    // Build columns from pre-computed vectors (no re-transformation)
    let enriched = build_enrichment_columns(&work_df, &vectors, &model, 5)?;

    let mut final_df = enriched;
    drop(final_df.drop_in_place("_combined_text"));

    // Save model if path provided
    if let Some(path) = model_path {
        model.save(path)?;
    }

    Ok((final_df, model))
}

fn fit_new_model(
    work_df: &DataFrame,
    num_clusters: usize,
) -> Result<(TextEnrichmentModel, Vec<SparseVec>)> {
    let has_labels = work_df
        .column("label_names")
        .ok()
        .and_then(|c| {
            c.as_materialized_series()
                .str()
                .ok()
                .map(|ca| ca.into_iter().any(|v| v.is_some_and(|s| !s.is_empty())))
        })
        .unwrap_or(false);

    let config = EnrichmentConfig {
        text_col: "_combined_text".to_string(),
        label_col: if has_labels {
            Some("label_names".to_string())
        } else {
            None
        },
        num_clusters,
        ..EnrichmentConfig::default()
    };

    fit_enrichment_model(work_df, &config)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_issue_df() -> DataFrame {
        DataFrame::new(vec![
            Column::new(
                "title".into(),
                vec![
                    "memory leak in parser",
                    "crash on startup",
                    "add dark mode",
                    "improve API docs",
                    "null pointer in handler",
                    "new dashboard widget",
                    "fix segfault on close",
                    "add export feature",
                    "buffer overflow in reader",
                    "redesign settings page",
                ],
            ),
            Column::new(
                "body".into(),
                vec![
                    "the parser leaks memory when processing large JSON files",
                    "application crashes immediately on launch with invalid config",
                    "users have requested a dark theme for the application",
                    "the API documentation is incomplete and needs examples",
                    "getting null pointer dereference in the request handler",
                    "would like a new widget showing recent activity on dashboard",
                    "segmentation fault when closing multiple tabs simultaneously",
                    "need ability to export data as CSV and JSON formats",
                    "buffer overflow detected when reading large binary files",
                    "the settings page layout needs a complete visual refresh",
                ],
            ),
            Column::new(
                "label_names".into(),
                vec![
                    "bug", "bug", "feature", "feature", "bug", "feature", "bug", "feature", "bug",
                    "feature",
                ],
            ),
        ])
        .unwrap()
    }

    #[test]
    fn test_enrich_github_issues() {
        let df = make_issue_df();
        let (enriched, model) = enrich_github_issues(&df, None, 3).unwrap();

        assert!(enriched.column("topic_terms").is_ok());
        assert!(enriched.column("topic_cluster").is_ok());
        assert!(enriched.column("predicted_label").is_ok());
        assert!(enriched.column("confidence").is_ok());
        assert_eq!(enriched.height(), df.height());

        assert!(enriched.column("_combined_text").is_err());
        assert!(enriched.column("_tfidf_vec").is_err());

        assert!(!model.label_centroids.is_empty());
        assert!(!model.cluster_centroids.is_empty());
        assert_eq!(model.cluster_names.len(), model.cluster_centroids.len());
    }

    #[test]
    fn test_model_persistence() {
        let df = make_issue_df();
        let (_, model) = enrich_github_issues(&df, None, 2).unwrap();

        let tmp = std::env::temp_dir().join("subtext_test_model.bin");
        model.save(&tmp).unwrap();
        let loaded = TextEnrichmentModel::load(&tmp).unwrap();

        assert_eq!(model.label_centroids.len(), loaded.label_centroids.len());
        assert_eq!(model.cluster_names, loaded.cluster_names);
        assert_eq!(
            model.cluster_centroids.len(),
            loaded.cluster_centroids.len()
        );

        drop(std::fs::remove_file(&tmp));
    }

    #[test]
    fn test_enrich_without_labels() {
        let df = DataFrame::new(vec![
            Column::new(
                "title".into(),
                vec!["memory leak", "add feature", "fix crash"],
            ),
            Column::new(
                "body".into(),
                vec!["leaking memory", "new feature", "crash fix"],
            ),
            Column::new("label_names".into(), vec!["", "", ""]),
        ])
        .unwrap();

        let (enriched, model) = enrich_github_issues(&df, None, 2).unwrap();

        assert!(enriched.column("topic_terms").is_ok());
        assert!(enriched.column("topic_cluster").is_ok());
        assert!(model.label_centroids.is_empty());
    }
}

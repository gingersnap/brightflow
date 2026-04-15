use std::collections::HashMap;

use polars::prelude::*;

use subtext::polars::{
    build_label_centroids, nearest_label, tfidf_fit, tfidf_transform, TfIdfConfig,
};
use subtext::{FittedTfIdf, SparseVec, TokenizerPreset};

/// Result of text classification on a DataFrame.
pub struct TextClassificationResult {
    /// The fitted TF-IDF model (can be serialized for reuse).
    pub fitted: FittedTfIdf,
    /// Label centroids (can be serialized for reuse).
    pub centroids: HashMap<String, SparseVec>,
    /// The DataFrame with added `predicted_label` and `confidence` columns.
    pub df: DataFrame,
}

/// Classify text rows in a DataFrame by label similarity.
///
/// Expects a DataFrame with:
/// - `text_col`: a String column containing the text to classify
/// - `label_col`: a String column with comma-separated labels (for building centroids)
///
/// Rows with non-empty labels are used to build centroids.
/// All rows are then classified against those centroids.
///
/// Returns the original DataFrame with `predicted_label` and `confidence` columns appended.
pub fn classify_by_labels(
    df: &DataFrame,
    text_col: &str,
    label_col: &str,
) -> Result<TextClassificationResult, Box<dyn std::error::Error>> {
    let text_series = df.column(text_col)?.as_materialized_series();

    // Fit TF-IDF on the text column
    // Adapt min_df to corpus size: use absolute count for small corpora
    let num_docs = text_series.len();
    let min_df = if num_docs >= 10 { 2.0 } else { 1.0 };

    let config = TfIdfConfig {
        ngram_range: 1..=2,
        min_df,
        sublinear_tf: true,
        preset: TokenizerPreset::CodeAware,
        ..TfIdfConfig::default()
    };
    let fitted = tfidf_fit(text_series, config)?;

    // Build centroids from labeled rows
    let centroids = build_label_centroids(df, text_col, label_col, &fitted)?;

    if centroids.is_empty() {
        return Err("no centroids built — no labeled rows found".into());
    }

    // Transform all rows
    let transformed = tfidf_transform(text_series, &fitted)?;

    // Classify
    let (label_series, score_series) = nearest_label(&transformed, &centroids)?;

    // Append columns to DataFrame
    let result_df = df
        .clone()
        .with_column(label_series)?
        .with_column(score_series)?
        .clone();

    Ok(TextClassificationResult {
        fitted,
        centroids,
        df: result_df,
    })
}

/// Classify GitHub issues using title + body as text, label_names as labels.
///
/// Combines `title` and `body` into a single text field, then classifies.
pub fn classify_github_issues(
    df: &DataFrame,
) -> Result<TextClassificationResult, Box<dyn std::error::Error>> {
    // Combine title and body into a single text column
    let title = df.column("title")?.as_materialized_series().str()?;
    let body = df.column("body")?.as_materialized_series().str()?;

    let combined: StringChunked = title
        .into_iter()
        .zip(body)
        .map(|(t, b)| {
            let title_str = t.unwrap_or("");
            let body_str = b.unwrap_or("");
            Some(format!("{title_str} {body_str}"))
        })
        .collect();

    let combined_series = combined.with_name("text".into()).into_series();

    // Build working DataFrame with combined text + labels
    let work_df = DataFrame::new(vec![
        combined_series.into_column(),
        df.column("label_names")?.clone(),
    ])?;

    classify_by_labels(&work_df, "text", "label_names")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use subtext::cosine;

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
    fn test_classify_github_issues() {
        let df = make_issue_df();
        let result = classify_github_issues(&df).unwrap();

        // Should have 2 centroids
        assert_eq!(result.centroids.len(), 2);
        assert!(result.centroids.contains_key("bug"));
        assert!(result.centroids.contains_key("feature"));

        // Result DF should have the extra columns
        assert!(result.df.column("predicted_label").is_ok());
        assert!(result.df.column("confidence").is_ok());
        assert_eq!(result.df.height(), df.height());

        // Check predictions are reasonable
        let labels = result
            .df
            .column("predicted_label")
            .unwrap()
            .as_materialized_series()
            .str()
            .unwrap();
        let confidences = result
            .df
            .column("confidence")
            .unwrap()
            .as_materialized_series()
            .f32()
            .unwrap();

        for (opt_label, opt_score) in labels.into_iter().zip(confidences.into_iter()) {
            let label = opt_label.unwrap_or("");
            assert!(
                label == "bug" || label == "feature",
                "unexpected label: {label}"
            );
            let score = opt_score.unwrap_or(0.0);
            assert!(score >= 0.0, "negative score: {score}");
        }
    }

    #[test]
    fn test_classify_by_labels() {
        let df = DataFrame::new(vec![
            Column::new(
                "text".into(),
                vec![
                    "server error 500 internal",
                    "new feature for users",
                    "performance regression",
                ],
            ),
            Column::new("category".into(), vec!["error", "feature", "error"]),
        ])
        .unwrap();

        let result = classify_by_labels(&df, "text", "category").unwrap();
        assert_eq!(result.centroids.len(), 2);
        assert_eq!(result.df.height(), 3);
    }

    #[test]
    fn test_centroids_similarity() {
        let df = make_issue_df();
        let result = classify_github_issues(&df).unwrap();

        let bug_centroid = &result.centroids["bug"];
        let feature_centroid = &result.centroids["feature"];

        // Bug and feature centroids should be somewhat different
        let similarity = cosine(bug_centroid, feature_centroid);
        assert!(similarity < 0.9, "centroids too similar: {similarity}");
    }
}

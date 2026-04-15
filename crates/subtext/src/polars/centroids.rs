use std::collections::HashMap;

use polars::prelude::*;

use crate::error::{Result, SubtextError};
use crate::{FittedTfIdf, SparseVec};

/// Build label centroids from a DataFrame with text and label columns.
///
/// For multi-label rows (comma-separated labels in `label_col`), each document
/// contributes to every label it belongs to. The resulting centroids are
/// L2-normalized.
pub fn build_label_centroids(
    df: &DataFrame,
    text_col: &str,
    label_col: &str,
    fitted: &FittedTfIdf,
) -> Result<HashMap<String, SparseVec>> {
    let text_series = df
        .column(text_col)
        .map_err(|_| SubtextError::ColumnNotFound(text_col.to_string()))?
        .as_materialized_series();
    let label_series = df
        .column(label_col)
        .map_err(|_| SubtextError::ColumnNotFound(label_col.to_string()))?
        .as_materialized_series();

    let text_ca = text_series.str().map_err(|_| SubtextError::TypeMismatch {
        expected: "String".to_string(),
        actual: format!("{:?}", text_series.dtype()),
    })?;
    let label_ca = label_series.str().map_err(|_| SubtextError::TypeMismatch {
        expected: "String".to_string(),
        actual: format!("{:?}", label_series.dtype()),
    })?;

    let mut accumulators: HashMap<String, (SparseVec, u32)> = HashMap::new();

    for (opt_text, opt_labels) in text_ca.into_iter().zip(label_ca) {
        let text = match opt_text {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };
        let labels = match opt_labels {
            Some(l) if !l.is_empty() => l,
            _ => continue,
        };

        let vec = fitted.transform(text);
        if vec.is_empty() {
            continue;
        }

        for label in labels.split(',') {
            let label = label.trim();
            if label.is_empty() {
                continue;
            }

            let entry = accumulators
                .entry(label.to_string())
                .or_insert_with(|| (SparseVec::empty(vec.generation()), 0));
            entry.0.add_assign(&vec);
            entry.1 += 1;
        }
    }

    let mut centroids = HashMap::with_capacity(accumulators.len());
    for (label, (mut sum, count)) in accumulators {
        if count > 0 {
            sum.scale(count as f32);
            sum.normalize();
            centroids.insert(label, sum);
        }
    }

    Ok(centroids)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{TfIdf, TokenizerPreset};

    fn make_test_df() -> (DataFrame, FittedTfIdf) {
        let texts = vec![
            "memory leak in processing pipeline",
            "null pointer crash on startup",
            "add dark mode theme option",
            "improve api documentation quality",
            "fix segfault when parsing json",
            "new feature request for dashboard",
        ];
        let labels = vec!["bug", "bug", "feature", "feature", "bug", "feature"];

        let df = DataFrame::new(vec![
            Column::new("text".into(), texts.clone()),
            Column::new("labels".into(), labels),
        ])
        .unwrap();

        let fitted = TfIdf::new()
            .preset(TokenizerPreset::Plain)
            .ngram_range(1..=2)
            .fit(&texts)
            .unwrap();

        (df, fitted)
    }

    #[test]
    fn test_build_centroids_basic() {
        let (df, fitted) = make_test_df();
        let centroids = build_label_centroids(&df, "text", "labels", &fitted).unwrap();

        assert_eq!(centroids.len(), 2);
        assert!(centroids.contains_key("bug"));
        assert!(centroids.contains_key("feature"));
    }

    #[test]
    fn test_centroids_are_normalized() {
        let (df, fitted) = make_test_df();
        let centroids = build_label_centroids(&df, "text", "labels", &fitted).unwrap();

        for (label, centroid) in &centroids {
            let norm = centroid.l2_norm();
            assert!(
                (norm - 1.0).abs() < 1e-4,
                "centroid '{label}' has norm {norm}"
            );
        }
    }

    #[test]
    fn test_multi_label() {
        let df = DataFrame::new(vec![
            Column::new(
                "text".into(),
                vec![
                    "fix memory leak bug",
                    "improve performance speed",
                    "crash and feature request",
                ],
            ),
            Column::new(
                "labels".into(),
                vec!["bug, performance", "performance", "bug, feature"],
            ),
        ])
        .unwrap();

        let texts: Vec<&str> = vec![
            "fix memory leak bug",
            "improve performance speed",
            "crash and feature request",
        ];
        let fitted = TfIdf::new().fit(&texts).unwrap();

        let centroids = build_label_centroids(&df, "text", "labels", &fitted).unwrap();

        assert_eq!(centroids.len(), 3);
        assert!(centroids.contains_key("bug"));
        assert!(centroids.contains_key("performance"));
        assert!(centroids.contains_key("feature"));
    }

    #[test]
    fn test_missing_column() {
        let (df, fitted) = make_test_df();
        let result = build_label_centroids(&df, "nonexistent", "labels", &fitted);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_dataframe() {
        let df = DataFrame::new(vec![
            Column::new("text".into(), Vec::<String>::new()),
            Column::new("labels".into(), Vec::<String>::new()),
        ])
        .unwrap();

        let fitted = TfIdf::new().fit(&["dummy text"]).unwrap();
        let centroids = build_label_centroids(&df, "text", "labels", &fitted).unwrap();
        assert!(centroids.is_empty());
    }
}

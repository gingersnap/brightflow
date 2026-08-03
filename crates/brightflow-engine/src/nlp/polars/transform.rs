//! DataFrame-level TF-IDF fit/transform and nearest-label assignment.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use polars::prelude::*;

use crate::nlp::error::{Result, SubtextError};
use crate::nlp::similarity::cosine;
use crate::nlp::{FittedTfIdf, SparseVec, TfIdf, TokenizerPreset};

use super::serde_utils::{sparse_vec_from_bytes, sparse_vec_to_bytes};

/// Configuration for TF-IDF fitting from a Polars Series.
pub struct TfIdfConfig {
    pub ngram_range: RangeInclusive<usize>,
    pub min_df: f32,
    pub max_df: f32,
    pub sublinear_tf: bool,
    pub preset: TokenizerPreset,
}

impl Default for TfIdfConfig {
    fn default() -> Self {
        Self {
            ngram_range: 1..=1,
            min_df: 1.0,
            max_df: 1.0,
            sublinear_tf: true,
            preset: TokenizerPreset::Plain,
        }
    }
}

/// Fit a TF-IDF model on a Utf8/String Series.
pub fn tfidf_fit(series: &Series, config: TfIdfConfig) -> Result<FittedTfIdf> {
    let ca = series.str().map_err(|_| SubtextError::TypeMismatch {
        expected: "String".to_string(),
        actual: format!("{:?}", series.dtype()),
    })?;

    let docs: Vec<&str> = ca.into_no_null_iter().collect();

    let fitted = TfIdf::new()
        .preset(config.preset)
        .ngram_range(config.ngram_range)
        .min_df(config.min_df)
        .max_df(config.max_df)
        .sublinear_tf(config.sublinear_tf)
        .fit(&docs)?;

    Ok(fitted)
}

/// Transform a Utf8/String Series into a Binary Series of serialized `SparseVec`s.
pub fn tfidf_transform(series: &Series, fitted: &FittedTfIdf) -> Result<Series> {
    let ca = series.str().map_err(|_| SubtextError::TypeMismatch {
        expected: "String".to_string(),
        actual: format!("{:?}", series.dtype()),
    })?;

    let name = series.name().clone();
    let byte_values: Vec<Option<Vec<u8>>> = ca
        .into_iter()
        .map(|opt_text| {
            let text = opt_text.unwrap_or("");
            let vec = fitted.transform(text);
            sparse_vec_to_bytes(&vec).ok()
        })
        .collect();

    let binary_ca = BinaryChunked::new(
        name,
        byte_values
            .iter()
            .map(|opt| opt.as_deref())
            .collect::<Vec<_>>(),
    );

    Ok(binary_ca.into_series())
}

/// Compute cosine similarity of each row against a target vector.
///
/// Takes a Binary Series (from `tfidf_transform`) and returns a Float32 Series.
pub fn cosine_similarity_column(col: &Series, target: &SparseVec) -> Result<Series> {
    let ca = col.binary().map_err(|_| SubtextError::TypeMismatch {
        expected: "Binary".to_string(),
        actual: format!("{:?}", col.dtype()),
    })?;

    let name = col.name().clone();
    let scores: Vec<Option<f32>> = ca
        .into_iter()
        .map(|opt_bytes| {
            opt_bytes.and_then(|bytes| {
                sparse_vec_from_bytes(bytes)
                    .ok()
                    .map(|vec| cosine(&vec, target))
            })
        })
        .collect();

    let series = Float32Chunked::new(name, scores).into_series();
    Ok(series)
}

/// Classify each row by finding the highest-cosine centroid.
///
/// Returns `(labels, scores)` — the best-matching label and its similarity score.
#[allow(clippy::explicit_into_iter_loop)]
pub fn nearest_label<S: std::hash::BuildHasher>(
    col: &Series,
    centroids: &HashMap<String, SparseVec, S>,
) -> Result<(Series, Series)> {
    let ca = col.binary().map_err(|_| SubtextError::TypeMismatch {
        expected: "Binary".to_string(),
        actual: format!("{:?}", col.dtype()),
    })?;

    let centroid_entries: Vec<(&str, &SparseVec)> =
        centroids.iter().map(|(k, v)| (k.as_str(), v)).collect();

    let mut labels: Vec<Option<String>> = Vec::with_capacity(ca.len());
    let mut scores: Vec<Option<f32>> = Vec::with_capacity(ca.len());

    for opt_bytes in ca.into_iter() {
        if let Some(vec) = opt_bytes.and_then(|b| sparse_vec_from_bytes(b).ok()) {
            let mut best_label: Option<&str> = None;
            let mut best_score = f32::NEG_INFINITY;

            for &(label, centroid) in &centroid_entries {
                let score = cosine(&vec, centroid);
                if score > best_score {
                    best_score = score;
                    best_label = Some(label);
                }
            }

            labels.push(best_label.map(String::from));
            scores.push(if best_score > f32::NEG_INFINITY {
                Some(best_score)
            } else {
                None
            });
        } else {
            labels.push(None);
            scores.push(None);
        }
    }

    let label_series = StringChunked::new("predicted_label".into(), labels).into_series();
    let score_series = Float32Chunked::new("confidence".into(), scores).into_series();

    Ok((label_series, score_series))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_series() -> Series {
        Series::new(
            "text".into(),
            vec![
                "memory leak when processing large files",
                "null pointer exception in parser module",
                "improve documentation for API endpoints",
                "add dark mode to settings page",
                "fix crash on startup with invalid config",
            ],
        )
    }

    #[test]
    fn test_tfidf_fit_transform_roundtrip() {
        let series = make_test_series();
        let fitted = tfidf_fit(&series, TfIdfConfig::default()).unwrap();
        let transformed = tfidf_transform(&series, &fitted).unwrap();

        assert_eq!(transformed.len(), series.len());
        assert_eq!(transformed.dtype(), &DataType::Binary);
    }

    #[test]
    fn test_cosine_similarity_column() {
        let series = make_test_series();
        let fitted = tfidf_fit(&series, TfIdfConfig::default()).unwrap();
        let transformed = tfidf_transform(&series, &fitted).unwrap();

        let target = fitted.transform("memory leak when processing large files");
        let scores = cosine_similarity_column(&transformed, &target).unwrap();

        assert_eq!(scores.len(), series.len());
        assert_eq!(scores.dtype(), &DataType::Float32);

        let score_values: Vec<f32> = scores.f32().unwrap().into_no_null_iter().collect();

        let max_idx = score_values
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(max_idx, 0, "first doc should be most similar to itself");
    }

    #[test]
    fn test_nearest_label() {
        let series = make_test_series();
        let config = TfIdfConfig {
            ngram_range: 1..=2,
            ..TfIdfConfig::default()
        };
        let fitted = tfidf_fit(&series, config).unwrap();
        let transformed = tfidf_transform(&series, &fitted).unwrap();

        let mut centroids = HashMap::new();
        let bug_vec = fitted.transform("crash error exception null pointer memory leak fix bug");
        centroids.insert("bug".to_string(), bug_vec);
        let feature_vec = fitted.transform("add improve new feature dark mode settings");
        centroids.insert("feature".to_string(), feature_vec);

        let (labels, scores) = nearest_label(&transformed, &centroids).unwrap();

        assert_eq!(labels.len(), series.len());
        assert_eq!(scores.len(), series.len());
        assert_eq!(labels.dtype(), &DataType::String);
        assert_eq!(scores.dtype(), &DataType::Float32);

        let label_ca = labels.str().unwrap();
        for opt_label in label_ca {
            let label = opt_label.unwrap_or("");
            assert!(
                label == "bug" || label == "feature",
                "unexpected label: {label}"
            );
        }
    }

    #[test]
    fn test_type_mismatch_fit() {
        let series = Series::new("nums".into(), vec![1i32, 2, 3]);
        let result = tfidf_fit(&series, TfIdfConfig::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_type_mismatch_transform() {
        let text_series = make_test_series();
        let fitted = tfidf_fit(&text_series, TfIdfConfig::default()).unwrap();

        let int_series = Series::new("nums".into(), vec![1i32, 2, 3]);
        let result = tfidf_transform(&int_series, &fitted);
        assert!(result.is_err());
    }
}

//! Eval-only quality summaries behind `topics eval` / `topics eval-classifier`.
//!
//! Pure functions over vectors, assignments, and label targets, so the
//! numbers are testable without a store or an embedder. Everything returned
//! is display-ready — callers should format, not recompute.

use std::collections::{HashMap, HashSet};

use super::cluster_metrics::{davies_bouldin, npmi_coherence, silhouette};
use super::linear::{fit_centroid_baseline, fit_multilabel_linear, TrainOutcome};

/// Quality summary of one clustering result.
#[derive(Debug)]
pub struct ClusteringEval {
    pub silhouette: Option<f64>,
    pub davies_bouldin: Option<f64>,
    pub npmi: Option<f64>,
    pub unassigned_pct: f64,
}

/// Score a clustering.
///
/// Silhouette, Davies-Bouldin, NPMI coherence of quick per-cluster top terms
/// (in-cluster document frequency — eval-only naming, the real pipeline uses
/// c-TF-IDF), and the unassigned percentage.
#[must_use]
pub fn eval_clustering(
    vectors: &[Vec<f32>],
    assignments: &[Option<usize>],
    n_clusters: usize,
    texts: &[String],
) -> ClusteringEval {
    let unassigned_pct = assignments.iter().filter(|a| a.is_none()).count() as f64
        / assignments.len().max(1) as f64
        * 100.0;

    let mut term_df: Vec<HashMap<String, usize>> = vec![HashMap::new(); n_clusters];
    for (text, assigned) in texts.iter().zip(assignments.iter()) {
        let Some(c) = assigned else { continue };
        let Some(counts) = term_df.get_mut(*c) else {
            continue;
        };
        let mut seen = HashSet::new();
        for token in text.to_lowercase().split_whitespace() {
            if token.len() > 3 && seen.insert(token.to_string()) {
                *counts.entry(token.to_string()).or_insert(0) += 1;
            }
        }
    }
    let cluster_terms: Vec<Vec<String>> = term_df
        .iter()
        .map(|counts| {
            let mut ranked: Vec<(&String, &usize)> = counts.iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(a.1));
            ranked.into_iter().take(6).map(|(t, _)| t.clone()).collect()
        })
        .collect();

    ClusteringEval {
        silhouette: silhouette(vectors, assignments),
        davies_bouldin: davies_bouldin(vectors, assignments),
        npmi: npmi_coherence(texts, &cluster_terms),
        unassigned_pct,
    }
}

/// Head-vs-baseline comparison for the supervised classifier.
#[derive(Debug)]
pub struct ClassifierEval {
    /// The trained linear head and its validation macro-F1.
    pub outcome: TrainOutcome,
    /// Macro-F1 of the nearest-centroid baseline over the head's retained
    /// labels; None when the baseline could not be fitted.
    pub baseline: Option<f32>,
}

/// Train the linear head and the nearest-centroid baseline on the same rows.
/// None = too little labelled signal to train (same policy as `fit_topics`).
#[must_use]
pub fn classifier_eval(
    features: &[Vec<f32>],
    row_targets: &[Vec<usize>],
    n_labels: usize,
    dim: usize,
) -> Option<ClassifierEval> {
    let outcome = fit_multilabel_linear(features, row_targets, n_labels, dim)?;
    let baseline = fit_centroid_baseline(features, row_targets, &outcome.retained_labels, dim);
    Some(ClassifierEval { outcome, baseline })
}

/// Pair rows that have BOTH a usable embedding and at least one label — the
/// same eligibility rule `fit_topics` applies before training the head.
#[must_use]
pub fn labelled_feature_rows(
    embeddings: &[Option<Vec<f32>>],
    per_row: &[Vec<usize>],
    dim: usize,
) -> (Vec<Vec<f32>>, Vec<Vec<usize>>) {
    let mut features: Vec<Vec<f32>> = Vec::new();
    let mut row_targets: Vec<Vec<usize>> = Vec::new();
    for (i, ids) in per_row.iter().enumerate() {
        if ids.is_empty() {
            continue;
        }
        if let Some(Some(v)) = embeddings.get(i) {
            if v.len() == dim {
                features.push(v.clone());
                row_targets.push(ids.clone());
            }
        }
    }
    (features, row_targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Blobs = (Vec<Vec<f32>>, Vec<Option<usize>>, Vec<String>);

    fn blobs() -> Blobs {
        let mut vectors = Vec::new();
        let mut assignments = Vec::new();
        let mut texts = Vec::new();
        for i in 0..20 {
            let j = (i % 5) as f32 * 0.01;
            vectors.push(vec![1.0, j, 0.0]);
            assignments.push(Some(0));
            texts.push("alpha bravo charlie".to_string());
            vectors.push(vec![0.0, j, 1.0]);
            assignments.push(Some(1));
            texts.push("delta echoes foxtrot".to_string());
        }
        (vectors, assignments, texts)
    }

    #[test]
    fn eval_clustering_scores_separated_blobs() {
        let (v, mut a, t) = blobs();
        a[0] = None; // one noise point
        let eval = eval_clustering(&v, &a, 2, &t);
        assert!(eval.silhouette.unwrap() > 0.7, "{eval:?}");
        assert!(eval.davies_bouldin.unwrap() < 1.0, "{eval:?}");
        assert!(eval.npmi.is_some(), "coherent per-cluster terms: {eval:?}");
        assert!((eval.unassigned_pct - 2.5).abs() < 1e-9, "1/40 unassigned");
    }

    #[test]
    fn eval_clustering_empty_input_does_not_panic() {
        let eval = eval_clustering(&[], &[], 0, &[]);
        assert!((eval.unassigned_pct - 0.0).abs() < 1e-9);
        assert!(eval.silhouette.is_none());
    }

    #[test]
    fn classifier_eval_trains_on_separable_synthetic_rows() {
        // 80 rows, 2 labels, cleanly separable along the two dims.
        let mut features = Vec::new();
        let mut targets = Vec::new();
        for i in 0..80 {
            let jitter = (i % 7) as f32 * 0.01;
            if i % 2 == 0 {
                features.push(vec![1.0 + jitter, 0.0]);
                targets.push(vec![0]);
            } else {
                features.push(vec![0.0, 1.0 + jitter]);
                targets.push(vec![1]);
            }
        }
        let eval = classifier_eval(&features, &targets, 2, 2).expect("enough signal");
        assert!(eval.outcome.val_macro_f1 > 0.9, "{:?}", eval.outcome);
        assert_eq!(eval.outcome.retained_labels.len(), 2);
        assert!(eval.baseline.is_some());
    }

    #[test]
    fn classifier_eval_refuses_thin_signal() {
        let features = vec![vec![1.0, 0.0]; 5];
        let targets = vec![vec![0]; 5];
        assert!(classifier_eval(&features, &targets, 1, 2).is_none());
    }

    #[test]
    fn labelled_feature_rows_pairs_rows_with_both() {
        let embeddings = vec![
            Some(vec![1.0, 0.0]), // labelled + embedded → kept
            None,                 // labelled, no embedding → dropped
            Some(vec![0.0, 1.0]), // unlabelled → dropped
            Some(vec![1.0]),      // wrong dim → dropped
        ];
        let per_row = vec![vec![0], vec![1], vec![], vec![0]];
        let (features, targets) = labelled_feature_rows(&embeddings, &per_row, 2);
        assert_eq!(features, vec![vec![1.0, 0.0]]);
        assert_eq!(targets, vec![vec![0]]);
    }
}

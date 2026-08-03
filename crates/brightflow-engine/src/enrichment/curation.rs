//! Reconciliation of curation edits onto freshly fitted clusters.
//!
//! `fit_topics` regenerates `clusters.bin` wholesale, so a cluster edit
//! ("rename cluster 3") cannot key on the cluster id. Edits attach to a
//! snapshot of the centroid they were made against; after each re-fit the
//! edit is re-pointed at the most similar new centroid (cosine ≥ 0.80) or
//! flagged orphaned for review. An embedder change orphans everything by
//! design — the geometry is incomparable.

use crate::nlp::fingerprint::fingerprint;

/// Cosine floor for re-attaching an edit to a new centroid.
pub const RECONCILE_MIN_COSINE: f32 = 0.80;

/// One stored edit's centroid snapshot.
#[derive(Debug, Clone)]
pub struct EditCentroid {
    pub edit_id: i64,
    pub centroid: Vec<f32>,
}

/// Where an edit lands after a re-fit.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileOutcome {
    pub edit_id: i64,
    /// New raw cluster id, or None = orphaned.
    pub new_cluster_id: Option<usize>,
    /// Cosine to the matched centroid (0.0 when orphaned).
    pub cosine: f32,
}

/// Pure reconciliation: match every edit to its best new centroid.
/// Dimension mismatches (embedder switched) orphan the edit.
pub fn reconcile_edits(
    edits: &[EditCentroid],
    new_centroids: &[Vec<f32>],
    min_cosine: f32,
) -> Vec<ReconcileOutcome> {
    edits
        .iter()
        .map(|edit| {
            let mut best: Option<(usize, f32)> = None;
            for (i, centroid) in new_centroids.iter().enumerate() {
                if centroid.len() != edit.centroid.len() {
                    continue; // incomparable geometry (embedder change)
                }
                let sim =
                    crate::nlp::similarity::dense_cosine_unnormalized(&edit.centroid, centroid);
                if best.is_none_or(|(_, b)| sim > b) {
                    best = Some((i, sim));
                }
            }
            match best {
                Some((idx, sim)) if sim >= min_cosine => ReconcileOutcome {
                    edit_id: edit.edit_id,
                    new_cluster_id: Some(idx),
                    cosine: sim,
                },
                _ => ReconcileOutcome {
                    edit_id: edit.edit_id,
                    new_cluster_id: None,
                    cosine: 0.0,
                },
            }
        })
        .collect()
}

/// Stable fingerprint of a centroid vector (quantized so float noise from
/// serialization round-trips doesn't change the key).
pub fn centroid_fingerprint(centroid: &[f32]) -> String {
    let quantized: Vec<String> = centroid.iter().map(|v| format!("{v:.4}")).collect();
    let joined = quantized.join(",");
    fingerprint(&["centroid", &joined])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: &[f32]) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / n).collect()
    }

    #[test]
    fn edit_reattaches_to_similar_centroid() {
        let edits = vec![EditCentroid {
            edit_id: 1,
            centroid: unit(&[1.0, 0.1, 0.0]),
        }];
        let new = vec![unit(&[0.0, 0.0, 1.0]), unit(&[1.0, 0.05, 0.02])];
        let out = reconcile_edits(&edits, &new, RECONCILE_MIN_COSINE);
        assert_eq!(out[0].new_cluster_id, Some(1));
        assert!(out[0].cosine > 0.99);
    }

    #[test]
    fn dissimilar_edit_is_orphaned() {
        let edits = vec![EditCentroid {
            edit_id: 7,
            centroid: unit(&[1.0, 0.0, 0.0]),
        }];
        let new = vec![unit(&[0.0, 1.0, 0.0]), unit(&[0.0, 0.0, 1.0])];
        let out = reconcile_edits(&edits, &new, RECONCILE_MIN_COSINE);
        assert_eq!(out[0].new_cluster_id, None);
    }

    #[test]
    fn embedder_change_orphans_by_dimension_mismatch() {
        let edits = vec![EditCentroid {
            edit_id: 2,
            centroid: unit(&[1.0, 0.0, 0.0, 0.0]), // 4-dim
        }];
        let new = vec![unit(&[1.0, 0.0, 0.0])]; // 3-dim
        let out = reconcile_edits(&edits, &new, RECONCILE_MIN_COSINE);
        assert_eq!(out[0].new_cluster_id, None);
    }

    #[test]
    fn centroid_fingerprint_stable_under_noise() {
        let a = vec![0.123_41_f32, 0.5];
        let b = vec![0.123_44_f32, 0.5]; // same at 4 decimals
        assert_eq!(centroid_fingerprint(&a), centroid_fingerprint(&b));
    }
}

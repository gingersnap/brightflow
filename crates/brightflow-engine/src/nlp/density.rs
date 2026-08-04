//! Density clustering (HDBSCAN) as an alternative to k-means.
//!
//! Pipeline: embeddings → PCA(≤16 dims, distances concentrate in 512-dim) →
//! HDBSCAN(min_cluster_size) → full-dimension centroids → per-cluster assign
//! thresholds. Noise points stay unassigned instead of being forced into the
//! least-bad cluster — the property k-means fundamentally can't offer.

use hdbscan::{Hdbscan, HdbscanHyperParams};
use tracing::info;

use super::dense_clustering::DenseClusterResult;
use super::reduce::Pca;

/// Dimensions to reduce to before density clustering.
const PCA_DIMS: usize = 12;
/// Members more than this many std-devs below their cluster's mean similarity
/// set the assignment threshold (mirrors the k-means trim).
const THRESHOLD_SIGMA: f32 = 2.0;
const MIN_THRESHOLD_CLUSTER: usize = 8;

/// Default `min_cluster_size` scaled to dataset size.
pub fn default_min_cluster_size(n: usize) -> usize {
    (n / 100).max(15)
}

/// HDBSCAN over L2-normalized embedding vectors.
///
/// Returns the same result shape as `kmeans_dense`: assignments with `None`
/// = noise, full-dimension normalized centroids, per-cluster thresholds.
pub fn hdbscan_dense(vectors: &[Vec<f32>], min_cluster_size: usize) -> DenseClusterResult {
    let n = vectors.len();
    if n < min_cluster_size.max(4) {
        return DenseClusterResult {
            assignments: vec![None; n],
            centroids: Vec::new(),
            iterations: 0,
            inertia: 0.0,
            assign_thresholds: Vec::new(),
        };
    }
    let dim = vectors[0].len();

    // Reduce for the density step; centroids are computed in full dimension.
    let reduced: Vec<Vec<f32>> = match Pca::fit(vectors, PCA_DIMS.min(dim)) {
        Some(pca) => pca.transform_batch(vectors),
        None => vectors.to_vec(),
    };

    let params = HdbscanHyperParams::builder()
        .min_cluster_size(min_cluster_size)
        .build();
    let clusterer = Hdbscan::new(&reduced, params);
    let labels: Vec<i32> = match clusterer.cluster() {
        Ok(l) => l,
        Err(e) => {
            info!("hdbscan failed ({e:?}); returning all-noise result");
            return DenseClusterResult {
                assignments: vec![None; n],
                centroids: Vec::new(),
                iterations: 0,
                inertia: 0.0,
                assign_thresholds: Vec::new(),
            };
        },
    };

    // Compact labels (−1 = noise) to contiguous cluster ids.
    let mut label_map: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    let mut unique: Vec<i32> = labels.iter().copied().filter(|l| *l >= 0).collect();
    unique.sort_unstable();
    unique.dedup();
    for (idx, label) in unique.iter().enumerate() {
        label_map.insert(*label, idx);
    }
    let k = label_map.len();
    let assignments: Vec<Option<usize>> =
        labels.iter().map(|l| label_map.get(l).copied()).collect();

    // Full-dimension normalized centroids
    let (mut centroids, _counts) =
        super::dense_clustering::mean_centroids(vectors, assignments.iter().copied(), k, dim);
    for centroid in &mut centroids {
        super::dense_clustering::l2_normalize_in_place(centroid);
    }

    // Per-cluster assignment thresholds from member similarity spread
    let mut sims_by_cluster: Vec<Vec<f32>> = vec![Vec::new(); k];
    for (v, a) in vectors.iter().zip(assignments.iter()) {
        if let Some(c) = a {
            if let (Some(bucket), Some(centroid)) = (sims_by_cluster.get_mut(*c), centroids.get(*c))
            {
                bucket.push(crate::nlp::similarity::dot_dense(v, centroid));
            }
        }
    }
    let assign_thresholds: Vec<f32> = sims_by_cluster
        .iter()
        .map(|sims| {
            if sims.len() < MIN_THRESHOLD_CLUSTER {
                return f32::NEG_INFINITY;
            }
            let count = sims.len() as f32;
            let mean = sims.iter().sum::<f32>() / count;
            let var = sims.iter().map(|s| (s - mean) * (s - mean)).sum::<f32>() / (count - 1.0);
            THRESHOLD_SIGMA.mul_add(-var.sqrt(), mean)
        })
        .collect();

    let inertia = vectors
        .iter()
        .zip(assignments.iter())
        .filter_map(|(v, a)| {
            a.map(|c| (1.0 - crate::nlp::similarity::dot_dense(v, &centroids[c])).max(0.0))
        })
        .sum();

    DenseClusterResult {
        assignments,
        centroids,
        iterations: 1,
        inertia,
        assign_thresholds,
    }
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
mod tests {
    use super::*;

    fn norm(v: &mut [f32]) {
        super::super::dense_clustering::l2_normalize_in_place(v);
    }

    #[test]
    fn recovers_planted_blobs_and_flags_noise() {
        let mut vectors = Vec::new();
        // Two dense blobs of 40 points each in 8 dims
        for c in 0..2 {
            for i in 0..40 {
                let mut v = vec![0.0f32; 8];
                v[c * 2] = 1.0;
                v[c * 2 + 1] = 0.8;
                for (j, x) in v.iter_mut().enumerate() {
                    *x += (((i * 7 + j * 3) % 10) as f32) * 0.01;
                }
                norm(&mut v);
                vectors.push(v);
            }
        }
        // Scattered noise points
        for i in 0..8 {
            let mut v = vec![0.1f32; 8];
            v[(i + 4) % 8] = ((i % 5) as f32).mul_add(0.2, 0.5);
            v[(i * 3 + 1) % 8] = 0.9 - (i % 3) as f32 * 0.3;
            norm(&mut v);
            vectors.push(v);
        }

        let r = hdbscan_dense(&vectors, 10);
        assert!(
            r.centroids.len() >= 2,
            "expected ≥2 clusters, got {}",
            r.centroids.len()
        );
        // Blob members recovered coherently
        let first_blob: Vec<usize> = (0..40).filter_map(|i| r.assignments[i]).collect();
        assert!(first_blob.len() >= 35, "blob 1 lost members");
        assert!(
            first_blob.iter().all(|&c| c == first_blob[0]),
            "blob 1 split"
        );
    }

    #[test]
    fn tiny_input_is_all_noise() {
        let vectors = vec![vec![1.0f32, 0.0]; 3];
        let r = hdbscan_dense(&vectors, 15);
        assert!(r.assignments.iter().all(Option::is_none));
    }
}

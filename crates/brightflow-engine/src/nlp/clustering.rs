//! K-means over sparse vectors, using cosine similarity.
//!
//! Cosine rather than Euclidean because document length should not determine
//! cluster membership — a short and a long document about the same subject point
//! the same direction, but are far apart by distance.

use super::similarity::cosine;
use super::sparse::SparseVec;

/// Result of k-means clustering over sparse vectors.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClusterResult {
    /// Cluster assignments for each input vector (index into `centroids`).
    pub assignments: Vec<usize>,
    /// Cluster centroids (L2-normalized).
    pub centroids: Vec<SparseVec>,
    /// Number of iterations run.
    pub iterations: usize,
}

/// Run k-means clustering over sparse TF-IDF vectors using cosine similarity.
///
/// Uses k-means++ initialization and iterates until convergence or `max_iter`.
/// Vectors should be L2-normalized (the default after TF-IDF transform) for
/// cosine distance to work correctly.
///
/// Internally uses dense centroid representation during iteration so that
/// each assignment is O(nnz_doc) instead of O(nnz_doc + nnz_centroid).
pub fn kmeans(vectors: &[SparseVec], k: usize, max_iter: usize) -> ClusterResult {
    if vectors.is_empty() || k == 0 {
        return ClusterResult {
            assignments: Vec::new(),
            centroids: Vec::new(),
            iterations: 0,
        };
    }

    let k = k.min(vectors.len());
    let generation = vectors.first().map_or(0, SparseVec::generation);

    // Find vocabulary size for dense representation.
    let vocab_size = vectors
        .iter()
        .filter_map(|v| v.indices().last().copied())
        .max()
        .map_or(0, |m| m as usize + 1);

    if vocab_size == 0 {
        return ClusterResult {
            assignments: vec![0; vectors.len()],
            centroids: (0..k).map(|_| SparseVec::empty(generation)).collect(),
            iterations: 0,
        };
    }

    // K-means++ initialization → immediately convert to dense centroids.
    // Dense centroids make assignment O(nnz_doc) per vector instead of
    // O(nnz_doc + nnz_centroid) with the sparse sorted-merge dot product.
    let init_sparse = kmeans_pp_init(vectors, k);
    let mut dense_centroids: Vec<Vec<f32>> = init_sparse
        .iter()
        .map(|sv| sparse_to_dense(sv, vocab_size))
        .collect();

    let mut assignments = vec![0usize; vectors.len()];
    let mut iterations = 0;

    for _ in 0..max_iter {
        iterations += 1;

        // Assign each vector to nearest dense centroid: O(N × K × nnz_doc)
        let mut changed = false;
        for (i, vec) in vectors.iter().enumerate() {
            let nearest = find_nearest_dense(vec, &dense_centroids);
            if nearest != assignments[i] {
                assignments[i] = nearest;
                changed = true;
            }
        }

        if !changed {
            break;
        }

        // Recompute centroids in dense form: O(N × nnz_doc), zero allocations
        recompute_dense(&mut dense_centroids, vectors, &assignments, k, vocab_size);
    }

    // Convert final dense centroids to sparse for the result.
    let centroids = dense_centroids
        .iter()
        .map(|dense| dense_to_sparse(dense, generation))
        .collect();

    ClusterResult {
        assignments,
        centroids,
        iterations,
    }
}

/// K-means++ initialization: pick diverse initial centroids.
fn kmeans_pp_init(vectors: &[SparseVec], k: usize) -> Vec<SparseVec> {
    let mut centroids = Vec::with_capacity(k);
    centroids.push(vectors[0].clone());

    for _ in 1..k {
        let mut best_idx = 0;
        let mut best_min_dist = f32::NEG_INFINITY;

        for (i, vec) in vectors.iter().enumerate() {
            let min_dist = centroids
                .iter()
                .map(|c| 1.0 - cosine(vec, c))
                .fold(f32::INFINITY, f32::min);

            if min_dist > best_min_dist {
                best_min_dist = min_dist;
                best_idx = i;
            }
        }

        centroids.push(vectors[best_idx].clone());
    }

    centroids
}

/// Dot product of a sparse vector against a dense array: O(nnz_sparse).
#[inline]
fn dot_sparse_dense(sparse: &SparseVec, dense: &[f32]) -> f32 {
    sparse.iter().fold(0.0_f32, |acc, (idx, val)| {
        val.mul_add(dense[idx as usize], acc)
    })
}

/// Find the nearest dense centroid by cosine similarity.
/// Since input vectors are L2-normalized, cosine = dot product.
#[inline]
fn find_nearest_dense(vec: &SparseVec, centroids: &[Vec<f32>]) -> usize {
    let mut best_idx = 0;
    let mut best_sim = f32::NEG_INFINITY;
    for (i, centroid) in centroids.iter().enumerate() {
        let sim = dot_sparse_dense(vec, centroid);
        if sim > best_sim {
            best_sim = sim;
            best_idx = i;
        }
    }
    best_idx
}

/// Recompute centroids in-place using dense accumulators.
/// Zero heap allocations in the hot loop.
fn recompute_dense(
    centroids: &mut [Vec<f32>],
    vectors: &[SparseVec],
    assignments: &[usize],
    k: usize,
    _vocab_size: usize,
) {
    // Zero out accumulators (reuse existing allocations).
    for c in centroids.iter_mut() {
        for v in c.iter_mut() {
            *v = 0.0;
        }
    }
    let mut counts = vec![0u32; k];

    // Accumulate (using f32 in-place; sufficient for centroid averaging).
    for (i, vec) in vectors.iter().enumerate() {
        let cluster = assignments[i];
        counts[cluster] += 1;
        let acc = &mut centroids[cluster];
        for (idx, val) in vec.iter() {
            acc[idx as usize] += val;
        }
    }

    // Average and L2-normalize in place.
    for (i, centroid) in centroids.iter_mut().enumerate() {
        if counts[i] == 0 {
            continue;
        }
        let count = counts[i] as f32;
        let mut norm_sq = 0.0_f32;
        for v in centroid.iter_mut() {
            *v /= count;
            norm_sq = v.mul_add(*v, norm_sq);
        }
        let norm = norm_sq.sqrt();
        if norm > 0.0 {
            for v in centroid.iter_mut() {
                *v /= norm;
            }
        }
    }
}

fn sparse_to_dense(sv: &SparseVec, size: usize) -> Vec<f32> {
    let mut dense = vec![0.0_f32; size];
    for (idx, val) in sv.iter() {
        dense[idx as usize] = val;
    }
    dense
}

fn dense_to_sparse(dense: &[f32], generation: u64) -> SparseVec {
    let mut indices = Vec::new();
    let mut values = Vec::new();
    for (idx, &val) in dense.iter().enumerate() {
        if val != 0.0 {
            indices.push(idx as u32);
            values.push(val);
        }
    }
    // Indices are guaranteed sorted (sequential iteration over dense array).
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    SparseVec::new(indices, values, generation).expect("dense-to-sparse indices are always sorted")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vectors() -> Vec<SparseVec> {
        // 6 vectors in 2 clear clusters:
        // Cluster A: high weight on indices 0,1
        // Cluster B: high weight on indices 5,6
        let mut vecs = vec![
            SparseVec::new(vec![0, 1], vec![1.0, 1.0], 0).unwrap(),
            SparseVec::new(vec![0, 1, 2], vec![1.0, 0.8, 0.2], 0).unwrap(),
            SparseVec::new(vec![0, 1, 3], vec![0.9, 1.0, 0.1], 0).unwrap(),
            SparseVec::new(vec![5, 6], vec![1.0, 1.0], 0).unwrap(),
            SparseVec::new(vec![5, 6, 7], vec![1.0, 0.8, 0.3], 0).unwrap(),
            SparseVec::new(vec![4, 5, 6], vec![0.1, 0.9, 1.0], 0).unwrap(),
        ];
        for v in &mut vecs {
            v.normalize();
        }
        vecs
    }

    #[test]
    fn test_kmeans_basic() {
        let vecs = make_vectors();
        let result = kmeans(&vecs, 2, 100);

        assert_eq!(result.assignments.len(), 6);
        assert_eq!(result.centroids.len(), 2);

        // First 3 should be in same cluster, last 3 in another
        assert_eq!(result.assignments[0], result.assignments[1]);
        assert_eq!(result.assignments[1], result.assignments[2]);
        assert_eq!(result.assignments[3], result.assignments[4]);
        assert_eq!(result.assignments[4], result.assignments[5]);
        assert_ne!(result.assignments[0], result.assignments[3]);
    }

    #[test]
    fn test_kmeans_single_cluster() {
        let vecs = make_vectors();
        let result = kmeans(&vecs, 1, 100);
        assert!(result.assignments.iter().all(|&a| a == 0));
        assert_eq!(result.centroids.len(), 1);
    }

    #[test]
    fn test_kmeans_empty() {
        let result = kmeans(&[], 3, 100);
        assert!(result.assignments.is_empty());
        assert!(result.centroids.is_empty());
    }

    #[test]
    fn test_kmeans_k_larger_than_n() {
        let vecs = make_vectors();
        let result = kmeans(&vecs, 100, 100);
        // k gets clamped to n=6
        assert_eq!(result.assignments.len(), 6);
    }

    #[test]
    fn test_centroids_normalized() {
        let vecs = make_vectors();
        let result = kmeans(&vecs, 2, 100);

        for centroid in &result.centroids {
            if !centroid.is_empty() {
                assert!(
                    (centroid.l2_norm() - 1.0).abs() < 1e-4,
                    "centroid not normalized: {}",
                    centroid.l2_norm()
                );
            }
        }
    }

    /// Stress test simulating the 47K GitHub issues workload.
    /// Run with: cargo test -p brightflow-engine --release -- test_kmeans_47k_stress --nocapture --ignored
    #[test]
    #[ignore = "stress test, run manually with --ignored"]
    fn test_kmeans_47k_stress() {
        use std::time::Instant;

        let n = 47_000;
        let vocab_size = 12_000;
        let avg_nnz = 40; // typical for TF-IDF on issue title+body
        let k = 8;
        let max_iter = 20;

        // Deterministic pseudo-random generator (simple LCG)
        let mut rng_state: u64 = 42;
        let mut next_u32 = || -> u32 {
            rng_state = rng_state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (rng_state >> 33) as u32
        };

        // Generate 47K sparse vectors with realistic sparsity
        eprintln!("generating {n} sparse vectors (vocab={vocab_size}, avg_nnz={avg_nnz})...");
        let t0 = Instant::now();
        let mut vectors: Vec<SparseVec> = Vec::with_capacity(n);
        for _ in 0..n {
            let nnz = (avg_nnz / 2) + (next_u32() as usize % avg_nnz);
            let mut indices: Vec<u32> = (0..nnz).map(|_| next_u32() % vocab_size as u32).collect();
            indices.sort_unstable();
            indices.dedup();
            let values: Vec<f32> = indices
                .iter()
                .map(|_| (next_u32() % 1000) as f32 / 1000.0 + 0.01)
                .collect();
            let mut v = SparseVec::new(indices, values, 0).unwrap();
            v.normalize();
            vectors.push(v);
        }
        eprintln!("  generated in {:?}", t0.elapsed());

        eprintln!("running kmeans(n={n}, k={k}, max_iter={max_iter})...");
        let t1 = Instant::now();
        let result = kmeans(&vectors, k, max_iter);
        let elapsed = t1.elapsed();
        eprintln!(
            "  kmeans completed in {elapsed:?} ({} iterations)",
            result.iterations
        );

        // Sanity checks
        assert_eq!(result.assignments.len(), n);
        assert_eq!(result.centroids.len(), k);
        for centroid in &result.centroids {
            if !centroid.is_empty() {
                assert!(
                    (centroid.l2_norm() - 1.0).abs() < 1e-3,
                    "centroid not normalized: {}",
                    centroid.l2_norm()
                );
            }
        }

        // Print cluster distribution
        let mut cluster_counts = vec![0usize; k];
        for &a in &result.assignments {
            cluster_counts[a] += 1;
        }
        for (i, count) in cluster_counts.iter().enumerate() {
            eprintln!("  cluster {i}: {count} vectors");
        }

        eprintln!("total wall time: {:?}", t0.elapsed());
    }
}

use crate::similarity::cosine;
use crate::sparse::SparseVec;

/// Result of k-means clustering over sparse vectors.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
pub fn kmeans(vectors: &[SparseVec], k: usize, max_iter: usize) -> ClusterResult {
    if vectors.is_empty() || k == 0 {
        return ClusterResult {
            assignments: Vec::new(),
            centroids: Vec::new(),
            iterations: 0,
        };
    }

    let k = k.min(vectors.len());

    // K-means++ initialization
    let mut centroids = kmeans_pp_init(vectors, k);
    let mut assignments = vec![0usize; vectors.len()];
    let mut iterations = 0;

    for _ in 0..max_iter {
        iterations += 1;

        // Assign each vector to nearest centroid
        let mut changed = false;
        for (i, vec) in vectors.iter().enumerate() {
            let nearest = find_nearest_centroid(vec, &centroids);
            if nearest != assignments[i] {
                assignments[i] = nearest;
                changed = true;
            }
        }

        if !changed {
            break;
        }

        // Recompute centroids
        centroids = recompute_centroids(vectors, &assignments, k);
    }

    ClusterResult {
        assignments,
        centroids,
        iterations,
    }
}

/// K-means++ initialization: pick diverse initial centroids.
fn kmeans_pp_init(vectors: &[SparseVec], k: usize) -> Vec<SparseVec> {
    let mut centroids = Vec::with_capacity(k);

    // First centroid: pick the vector closest to the corpus mean
    // (approximated by picking index 0 for simplicity — with normalized vectors
    // this works well enough)
    centroids.push(vectors[0].clone());

    // Remaining centroids: pick the vector with maximum minimum distance to existing centroids
    for _ in 1..k {
        let mut best_idx = 0;
        let mut best_min_dist = f32::NEG_INFINITY;

        for (i, vec) in vectors.iter().enumerate() {
            // Distance to nearest existing centroid (1 - cosine for normalized vectors)
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

/// Find the index of the nearest centroid by cosine similarity.
fn find_nearest_centroid(vec: &SparseVec, centroids: &[SparseVec]) -> usize {
    let mut best_idx = 0;
    let mut best_sim = f32::NEG_INFINITY;

    for (i, centroid) in centroids.iter().enumerate() {
        let sim = cosine(vec, centroid);
        if sim > best_sim {
            best_sim = sim;
            best_idx = i;
        }
    }

    best_idx
}

/// Recompute centroids by averaging assigned vectors and normalizing.
fn recompute_centroids(vectors: &[SparseVec], assignments: &[usize], k: usize) -> Vec<SparseVec> {
    let generation = vectors.first().map_or(0, SparseVec::generation);
    let mut centroids: Vec<SparseVec> = (0..k).map(|_| SparseVec::empty(generation)).collect();
    let mut counts = vec![0u32; k];

    for (i, vec) in vectors.iter().enumerate() {
        let cluster = assignments[i];
        centroids[cluster].add_assign(vec);
        counts[cluster] += 1;
    }

    for (i, centroid) in centroids.iter_mut().enumerate() {
        if counts[i] > 0 {
            centroid.scale(counts[i] as f32);
            centroid.normalize();
        }
    }

    centroids
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
}

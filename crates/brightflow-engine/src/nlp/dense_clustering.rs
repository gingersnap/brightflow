/// Result of dense k-means clustering over `Vec<f32>` vectors.
#[derive(Debug, Clone)]
pub struct DenseClusterResult {
    /// Cluster index for each input vector.
    pub assignments: Vec<usize>,
    /// L2-normalized centroids.
    pub centroids: Vec<Vec<f32>>,
    /// Number of iterations performed.
    pub iterations: usize,
}

/// K-means over L2-normalized dense vectors using cosine similarity.
///
/// Inputs are expected to be L2-normalized (Model2Vec returns normalized vectors
/// when `config.json` has `"normalize": true`, which the bundled model does).
/// With normalized vectors, cosine similarity equals the dot product.
pub fn kmeans_dense(vectors: &[Vec<f32>], k: usize, max_iter: usize) -> DenseClusterResult {
    if vectors.is_empty() || k == 0 {
        return DenseClusterResult {
            assignments: Vec::new(),
            centroids: Vec::new(),
            iterations: 0,
        };
    }

    let dim = vectors[0].len();
    let k = k.min(vectors.len());

    let mut centroids = kmeans_pp_init(vectors, k);
    let mut assignments = vec![0usize; vectors.len()];
    let mut iterations = 0;

    for _ in 0..max_iter {
        iterations += 1;

        let mut changed = false;
        for (i, vec) in vectors.iter().enumerate() {
            let nearest = nearest_centroid(vec, &centroids);
            if nearest != assignments[i] {
                assignments[i] = nearest;
                changed = true;
            }
        }

        recompute_centroids(&mut centroids, vectors, &assignments, dim);
        let reseeded = reseed_empty_clusters(&mut centroids, vectors, &mut assignments);
        if reseeded {
            // After reseeding, force the new centroid to "own" its seed and
            // any close vectors by recomputing once more.
            recompute_centroids(&mut centroids, vectors, &assignments, dim);
        }

        // Only stop when assignments stabilized AND no cluster is empty.
        if !changed && !reseeded {
            break;
        }
    }

    DenseClusterResult {
        assignments,
        centroids,
        iterations,
    }
}

/// K-means++ seeding: pick the vector furthest from existing centroids each round.
/// Deterministic — uses max-min distance rather than the standard probabilistic
/// variant so repeated fits on the same data give the same clusters.
fn kmeans_pp_init(vectors: &[Vec<f32>], k: usize) -> Vec<Vec<f32>> {
    let mut centroids: Vec<Vec<f32>> = Vec::with_capacity(k);
    centroids.push(vectors[0].clone());

    for _ in 1..k {
        let mut best_idx = 0;
        let mut best_min_dist = f32::NEG_INFINITY;
        for (i, vec) in vectors.iter().enumerate() {
            let min_dist = centroids
                .iter()
                .map(|c| 1.0 - dot(vec, c))
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

#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .fold(0.0_f32, |acc, (x, y)| x.mul_add(*y, acc))
}

fn nearest_centroid(vec: &[f32], centroids: &[Vec<f32>]) -> usize {
    let mut best_idx = 0;
    let mut best_sim = f32::NEG_INFINITY;
    for (i, centroid) in centroids.iter().enumerate() {
        let sim = dot(vec, centroid);
        if sim > best_sim {
            best_sim = sim;
            best_idx = i;
        }
    }
    best_idx
}

fn recompute_centroids(
    centroids: &mut [Vec<f32>],
    vectors: &[Vec<f32>],
    assignments: &[usize],
    dim: usize,
) {
    for c in centroids.iter_mut() {
        for v in c.iter_mut() {
            *v = 0.0;
        }
    }
    let mut counts = vec![0u32; centroids.len()];

    for (i, vec) in vectors.iter().enumerate() {
        let cluster = assignments[i];
        counts[cluster] += 1;
        let acc = &mut centroids[cluster];
        for (j, &val) in vec.iter().enumerate().take(dim) {
            acc[j] += val;
        }
    }

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

/// Cosine similarity for normalized vectors (dot product).
pub fn dense_cosine(a: &[f32], b: &[f32]) -> f32 {
    dot(a, b)
}

/// Detect empty clusters and re-seed each one from the data point that fits its
/// current cluster *least well* (smallest cosine to its assigned centroid).
/// Without this fix, an emptied cluster has a zero centroid and `dot(*, 0) == 0`,
/// so it can never recover assignments.
///
/// Returns `true` if any cluster was re-seeded.
fn reseed_empty_clusters(
    centroids: &mut [Vec<f32>],
    vectors: &[Vec<f32>],
    assignments: &mut [usize],
) -> bool {
    let mut counts = vec![0_u32; centroids.len()];
    for &a in assignments.iter() {
        counts[a] += 1;
    }

    let mut reseeded = false;
    for empty_idx in 0..centroids.len() {
        if counts[empty_idx] != 0 {
            continue;
        }
        // Find the worst-fit vector overall — the one with the lowest similarity
        // to its currently assigned centroid (and not from an already-tiny cluster).
        let mut worst_vec_idx: Option<usize> = None;
        let mut worst_sim = f32::INFINITY;
        for (i, vec) in vectors.iter().enumerate() {
            let cur_cluster = assignments[i];
            // Don't strip the last member from another cluster.
            if counts[cur_cluster] <= 1 {
                continue;
            }
            let sim = dot(vec, &centroids[cur_cluster]);
            if sim < worst_sim {
                worst_sim = sim;
                worst_vec_idx = Some(i);
            }
        }
        if let Some(i) = worst_vec_idx {
            // Move this point: copy as new centroid, update assignment + counts.
            centroids[empty_idx].copy_from_slice(&vectors[i]);
            let prev = assignments[i];
            assignments[i] = empty_idx;
            counts[prev] -= 1;
            counts[empty_idx] += 1;
            reseeded = true;
        }
    }
    reseeded
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn norm(v: &mut Vec<f32>) {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n > 0.0 {
            for x in v {
                *x /= n;
            }
        }
    }

    fn make_vectors() -> Vec<Vec<f32>> {
        let mut vecs = vec![
            vec![1.0, 1.0, 0.0, 0.0, 0.0],
            vec![1.0, 0.9, 0.1, 0.0, 0.0],
            vec![0.9, 1.0, 0.0, 0.1, 0.0],
            vec![0.0, 0.0, 0.0, 1.0, 1.0],
            vec![0.0, 0.0, 0.1, 1.0, 0.9],
            vec![0.0, 0.1, 0.0, 0.9, 1.0],
        ];
        for v in &mut vecs {
            norm(v);
        }
        vecs
    }

    #[test]
    fn test_kmeans_two_clusters() {
        let vecs = make_vectors();
        let r = kmeans_dense(&vecs, 2, 50);
        assert_eq!(r.centroids.len(), 2);
        assert_eq!(r.assignments[0], r.assignments[1]);
        assert_eq!(r.assignments[1], r.assignments[2]);
        assert_eq!(r.assignments[3], r.assignments[4]);
        assert_eq!(r.assignments[4], r.assignments[5]);
        assert_ne!(r.assignments[0], r.assignments[3]);
    }

    #[test]
    fn test_centroids_normalized() {
        let vecs = make_vectors();
        let r = kmeans_dense(&vecs, 2, 50);
        for c in &r.centroids {
            let n: f32 = c.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((n - 1.0).abs() < 1e-4, "centroid not normalized: {n}");
        }
    }

    #[test]
    fn test_empty() {
        let r = kmeans_dense(&[], 3, 10);
        assert!(r.assignments.is_empty());
        assert!(r.centroids.is_empty());
    }
}

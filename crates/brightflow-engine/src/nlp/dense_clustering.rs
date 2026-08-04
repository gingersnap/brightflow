//! Dense k-means over L2-normalized embedding vectors (cosine similarity).
//!
//! Fixes over the naive version this replaces:
//! - **True D² k-means++ seeding** (probabilistic, seeded PRNG) instead of
//!   deterministic max-min, which always planted centroids on the extreme
//!   outliers of the dataset.
//! - **`n_init` restarts** keeping the lowest-inertia run, so one unlucky
//!   seeding doesn't define the topics.
//! - **Empty-cluster reseeding from the largest cluster's farthest member**
//!   (splits the biggest blob) instead of adopting the globally worst-fit
//!   point (which re-planted centroids on outliers).
//! - **Post-convergence outlier trim**: members far below their cluster's
//!   typical similarity are unassigned (`None`) rather than polluting the
//!   cluster, and per-cluster assignment thresholds are exported so later
//!   rows are only attached when they genuinely fit.

use super::rng::SplitMix64;

/// Result of dense k-means clustering over `Vec<f32>` vectors.
#[derive(Debug, Clone)]
pub struct DenseClusterResult {
    /// Cluster index per input vector; `None` = trimmed outlier.
    pub assignments: Vec<Option<usize>>,
    /// L2-normalized centroids.
    pub centroids: Vec<Vec<f32>>,
    /// Number of Lloyd iterations of the winning restart.
    pub iterations: usize,
    /// Sum of cosine distances (1 − sim) of assigned members — lower is better.
    pub inertia: f32,
    /// Per-cluster minimum cosine similarity for assignment. New rows below
    /// the threshold should stay unassigned.
    pub assign_thresholds: Vec<f32>,
}

/// (assignments, centroids, iterations, inertia) of one k-means restart.
type RunOutcome = (Vec<usize>, Vec<Vec<f32>>, usize, f32);

const N_INIT: usize = 4;
/// Fixed base seed — clustering must be reproducible run-to-run.
const BASE_SEED: u64 = 0x00b1_1235_eed5_eed5;
/// Members more than this many std-devs below their cluster's mean similarity
/// are trimmed as outliers.
const TRIM_SIGMA: f32 = 2.0;
/// Clusters smaller than this are never trimmed (stats too noisy).
const MIN_TRIM_CLUSTER: usize = 8;

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
            inertia: 0.0,
            assign_thresholds: Vec::new(),
        };
    }

    let k = k.min(vectors.len());

    let mut best: Option<RunOutcome> = None;
    for restart in 0..N_INIT {
        let mut rng = SplitMix64::new(BASE_SEED.wrapping_add(restart as u64));
        let (assignments, centroids, iterations) = lloyd_run(vectors, k, max_iter, &mut rng);
        let inertia = compute_inertia(vectors, &assignments, &centroids);
        let better = best.as_ref().is_none_or(|(_, _, _, b)| inertia < *b);
        if better {
            best = Some((assignments, centroids, iterations, inertia));
        }
    }

    // `best` is always Some: N_INIT >= 1 and vectors is non-empty.
    let Some((assignments, centroids, iterations, inertia)) = best else {
        unreachable!("at least one k-means restart must run");
    };

    let (assignments, assign_thresholds) = trim_outliers(vectors, assignments, &centroids);

    DenseClusterResult {
        assignments,
        centroids,
        iterations,
        inertia,
        assign_thresholds,
    }
}

/// One full Lloyd run from a fresh D² seeding.
fn lloyd_run(
    vectors: &[Vec<f32>],
    k: usize,
    max_iter: usize,
    rng: &mut SplitMix64,
) -> (Vec<usize>, Vec<Vec<f32>>, usize) {
    let dim = vectors[0].len();
    let mut centroids = kmeans_pp_init(vectors, k, rng);
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

    (assignments, centroids, iterations)
}

/// True D² k-means++ seeding: first centroid uniform at random, each next
/// centroid sampled with probability proportional to its squared cosine
/// distance to the nearest already-chosen centroid.
fn kmeans_pp_init(vectors: &[Vec<f32>], k: usize, rng: &mut SplitMix64) -> Vec<Vec<f32>> {
    let n = vectors.len();
    let mut centroids: Vec<Vec<f32>> = Vec::with_capacity(k);
    let first = rng.next_bounded(n);
    centroids.push(vectors[first].clone());

    // min squared distance to any chosen centroid, updated incrementally
    let mut min_d2: Vec<f32> = vectors
        .iter()
        .map(|v| {
            let d = (1.0 - crate::nlp::similarity::dot_dense(v, &centroids[0])).max(0.0);
            d * d
        })
        .collect();

    for _ in 1..k {
        let total: f32 = min_d2.iter().sum();
        let idx = if total <= f32::EPSILON {
            // All points coincide with chosen centroids — pick uniformly.
            rng.next_bounded(n)
        } else {
            let mut target = rng.next_f32() * total;
            let mut chosen = n - 1;
            for (i, &d2) in min_d2.iter().enumerate() {
                target -= d2;
                if target <= 0.0 {
                    chosen = i;
                    break;
                }
            }
            chosen
        };
        let new_centroid = vectors[idx].clone();
        for (i, v) in vectors.iter().enumerate() {
            let d = (1.0 - crate::nlp::similarity::dot_dense(v, &new_centroid)).max(0.0);
            let d2 = d * d;
            if d2 < min_d2[i] {
                min_d2[i] = d2;
            }
        }
        centroids.push(new_centroid);
    }

    centroids
}

fn nearest_centroid(vec: &[f32], centroids: &[Vec<f32>]) -> usize {
    let mut best_idx = 0;
    let mut best_sim = f32::NEG_INFINITY;
    for (i, centroid) in centroids.iter().enumerate() {
        let sim = crate::nlp::similarity::dot_dense(vec, centroid);
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
    let (fresh, counts) = mean_centroids(
        vectors,
        assignments.iter().map(|&a| Some(a)),
        centroids.len(),
        dim,
    );
    for (dst, (mut src, n)) in centroids.iter_mut().zip(fresh.into_iter().zip(counts)) {
        if n > 0 {
            l2_normalize_in_place(&mut src);
        }
        // Empty clusters end up as zero vectors; reseed_empty_clusters
        // detects and replants them.
        *dst = src;
    }
}

/// Per-cluster mean vectors (and member counts) over `assignments`
/// (`Some(c)` = member of cluster `c`, `None` = unassigned/noise).
/// Empty clusters keep a zero vector; out-of-range indices are ignored.
pub(crate) fn mean_centroids(
    vectors: &[Vec<f32>],
    assignments: impl IntoIterator<Item = Option<usize>>,
    k: usize,
    dim: usize,
) -> (Vec<Vec<f32>>, Vec<usize>) {
    let mut centroids = vec![vec![0.0f32; dim]; k];
    let mut counts = vec![0usize; k];
    for (v, a) in vectors.iter().zip(assignments) {
        if let Some(c) = a {
            if let (Some(acc), Some(count)) = (centroids.get_mut(c), counts.get_mut(c)) {
                *count += 1;
                for (accx, x) in acc.iter_mut().zip(v.iter()) {
                    *accx += x;
                }
            }
        }
    }
    for (c, n) in centroids.iter_mut().zip(counts.iter()) {
        if *n > 0 {
            for x in c.iter_mut() {
                *x /= *n as f32;
            }
        }
    }
    (centroids, counts)
}

/// L2-normalize `v` in place; zero vectors are left untouched (the `> 0.0`
/// guard shared by every normalization site in this crate).
pub(crate) fn l2_normalize_in_place(v: &mut [f32]) {
    let norm = v.iter().fold(0.0_f32, |acc, x| x.mul_add(*x, acc)).sqrt();
    if norm > 0.0 {
        for x in v {
            *x /= norm;
        }
    }
}

/// Sum of cosine distances of every point to its assigned centroid.
fn compute_inertia(vectors: &[Vec<f32>], assignments: &[usize], centroids: &[Vec<f32>]) -> f32 {
    vectors
        .iter()
        .zip(assignments.iter())
        .map(|(v, &a)| (1.0 - crate::nlp::similarity::dot_dense(v, &centroids[a])).max(0.0))
        .sum()
}

/// Detect empty clusters and re-seed each from the *largest* cluster's member
/// farthest from its centroid — splitting the biggest blob instead of adopting
/// the globally worst-fit point (which planted centroids on outliers).
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
        // Largest cluster with at least 2 members
        let Some(largest) = (0..centroids.len())
            .filter(|&c| counts[c] >= 2)
            .max_by_key(|&c| counts[c])
        else {
            continue;
        };
        // Its farthest member becomes the new centroid's seed
        let mut worst_vec_idx: Option<usize> = None;
        let mut worst_sim = f32::INFINITY;
        for (i, vec) in vectors.iter().enumerate() {
            if assignments[i] != largest {
                continue;
            }
            let sim = crate::nlp::similarity::dot_dense(vec, &centroids[largest]);
            if sim < worst_sim {
                worst_sim = sim;
                worst_vec_idx = Some(i);
            }
        }
        if let Some(i) = worst_vec_idx {
            centroids[empty_idx].copy_from_slice(&vectors[i]);
            assignments[i] = empty_idx;
            counts[largest] -= 1;
            counts[empty_idx] += 1;
            reseeded = true;
        }
    }
    reseeded
}

/// Unassign members whose similarity to their centroid is far below the
/// cluster's typical similarity. Returns the (possibly trimmed) assignments
/// and the per-cluster assignment thresholds.
fn trim_outliers(
    vectors: &[Vec<f32>],
    assignments: Vec<usize>,
    centroids: &[Vec<f32>],
) -> (Vec<Option<usize>>, Vec<f32>) {
    let k = centroids.len();
    let mut sims_by_cluster: Vec<Vec<f32>> = vec![Vec::new(); k];
    let sims: Vec<f32> = vectors
        .iter()
        .zip(assignments.iter())
        .map(|(v, &a)| {
            let s = crate::nlp::similarity::dot_dense(v, &centroids[a]);
            sims_by_cluster[a].push(s);
            s
        })
        .collect();

    let thresholds: Vec<f32> = sims_by_cluster
        .iter()
        .map(|member_sims| {
            if member_sims.len() < MIN_TRIM_CLUSTER {
                return f32::NEG_INFINITY;
            }
            let n = member_sims.len() as f32;
            let mean = member_sims.iter().sum::<f32>() / n;
            let var = member_sims
                .iter()
                .map(|s| (s - mean) * (s - mean))
                .sum::<f32>()
                / (n - 1.0);
            TRIM_SIGMA.mul_add(-var.sqrt(), mean)
        })
        .collect();

    let assignments = assignments
        .into_iter()
        .zip(sims)
        .map(|(a, sim)| if sim < thresholds[a] { None } else { Some(a) })
        .collect();

    (assignments, thresholds)
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact expected values")]
mod tests {
    use super::*;

    fn norm(v: &mut [f32]) {
        l2_normalize_in_place(v);
    }

    #[test]
    fn mean_centroids_averages_members_and_counts() {
        let vectors = vec![
            vec![2.0, 0.0],
            vec![0.0, 2.0],
            vec![4.0, 0.0],
            vec![9.0, 9.0], // noise
        ];
        let assignments = [Some(0), Some(1), Some(0), None];
        let (centroids, counts) = mean_centroids(&vectors, assignments, 3, 2);
        assert_eq!(centroids[0], vec![3.0, 0.0]);
        assert_eq!(centroids[1], vec![0.0, 2.0]);
        assert_eq!(centroids[2], vec![0.0, 0.0], "empty cluster stays zero");
        assert_eq!(counts, vec![2, 1, 0]);
    }

    #[test]
    fn l2_normalize_in_place_unit_norm_and_zero_guard() {
        let mut v = vec![3.0f32, 4.0];
        l2_normalize_in_place(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);

        let mut z = vec![0.0f32, 0.0];
        l2_normalize_in_place(&mut z);
        assert_eq!(z, vec![0.0, 0.0], "zero vector must be left untouched");
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

    /// Planted clusters around orthogonal axes plus deterministic jitter.
    fn planted_clusters(per_cluster: usize, n_clusters: usize, dim: usize) -> Vec<Vec<f32>> {
        let mut rng = SplitMix64::new(42);
        let mut out = Vec::new();
        for c in 0..n_clusters {
            for _ in 0..per_cluster {
                let mut v = vec![0.0f32; dim];
                v[c] = 1.0;
                for x in &mut v {
                    *x = (rng.next_f32() - 0.5).mul_add(0.2, *x);
                }
                norm(&mut v);
                out.push(v);
            }
        }
        out
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
        // Tiny clusters are never trimmed
        assert!(r.assignments.iter().all(Option::is_some));
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

    #[test]
    fn deterministic_across_runs() {
        let vecs = planted_clusters(40, 3, 8);
        let a = kmeans_dense(&vecs, 3, 50);
        let b = kmeans_dense(&vecs, 3, 50);
        assert_eq!(a.assignments, b.assignments);
        assert_eq!(a.inertia, b.inertia);
    }

    #[test]
    fn recovers_planted_clusters_and_trims_noise() {
        let mut vecs = planted_clusters(50, 3, 8);
        // Inject noise points pointing into unused dimensions
        let mut rng = SplitMix64::new(7);
        let n_clean = vecs.len();
        for _ in 0..12 {
            let mut v = vec![0.0f32; 8];
            for x in v.iter_mut().skip(3) {
                *x = rng.next_f32() - 0.5;
            }
            norm(&mut v);
            vecs.push(v);
        }
        let r = kmeans_dense(&vecs, 3, 100);

        // Every planted cluster must be pure: all members of one plant share
        // an assignment (ignoring trimmed members).
        for c in 0..3 {
            let assigned: Vec<usize> = (c * 50..(c + 1) * 50)
                .filter_map(|i| r.assignments[i])
                .collect();
            assert!(
                assigned.len() >= 45,
                "cluster {c} lost too many members: {}",
                assigned.len()
            );
            let first = assigned[0];
            assert!(
                assigned.iter().all(|&a| a == first),
                "planted cluster {c} split"
            );
        }
        // The noise points should mostly be trimmed or at least not dominate
        let noise_assigned = (n_clean..vecs.len())
            .filter(|&i| r.assignments[i].is_some())
            .count();
        assert!(
            noise_assigned <= 6,
            "too many noise points kept: {noise_assigned}/12"
        );
    }

    #[test]
    fn thresholds_len_matches_k() {
        let vecs = planted_clusters(30, 2, 6);
        let r = kmeans_dense(&vecs, 2, 50);
        assert_eq!(r.assign_thresholds.len(), r.centroids.len());
    }
}

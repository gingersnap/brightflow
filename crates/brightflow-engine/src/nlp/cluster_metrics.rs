//! Clustering quality metrics for the eval harness (`brightflow topics eval`).
//!
//! These are the numbers that settle embedder and kmeans-vs-hdbscan
//! arguments on real data instead of vibes.

use std::collections::HashMap;

/// Deterministic sample cap for O(n²) metrics.
const SILHOUETTE_SAMPLE: usize = 2_000;

/// Mean silhouette coefficient over (sampled) assigned points, in [-1, 1].
/// Cosine distance; higher = tighter, better-separated clusters.
pub fn silhouette(vectors: &[Vec<f32>], assignments: &[Option<usize>]) -> Option<f64> {
    let assigned: Vec<(usize, usize)> = assignments
        .iter()
        .enumerate()
        .filter_map(|(i, a)| a.map(|c| (i, c)))
        .collect();
    if assigned.len() < 4 {
        return None;
    }
    let k = assigned.iter().map(|(_, c)| *c).max()? + 1;
    if k < 2 {
        return None;
    }

    // Deterministic stride sampling
    let stride = (assigned.len() / SILHOUETTE_SAMPLE).max(1);
    let sample: Vec<(usize, usize)> = assigned.iter().copied().step_by(stride).collect();

    let mut total = 0.0f64;
    let mut count = 0usize;
    for &(i, own) in &sample {
        let mut per_cluster: HashMap<usize, (f64, usize)> = HashMap::new();
        for &(j, cj) in &assigned {
            if i == j {
                continue;
            }
            let d = f64::from(cosine_distance(&vectors[i], &vectors[j]));
            let entry = per_cluster.entry(cj).or_insert((0.0, 0));
            entry.0 += d;
            entry.1 += 1;
        }
        let Some(&(own_sum, own_n)) = per_cluster.get(&own) else {
            continue;
        };
        if own_n == 0 {
            continue;
        }
        let a = own_sum / own_n as f64;
        let b = per_cluster
            .iter()
            .filter(|(c, _)| **c != own)
            .map(|(_, (sum, n))| sum / (*n).max(1) as f64)
            .fold(f64::INFINITY, f64::min);
        if !b.is_finite() {
            continue;
        }
        let s = (b - a) / a.max(b).max(1e-12);
        total += s;
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some(total / count as f64)
    }
}

/// Davies-Bouldin index (lower is better): mean over clusters of the worst
/// (scatter_i + scatter_j) / centroid_distance ratio.
pub fn davies_bouldin(vectors: &[Vec<f32>], assignments: &[Option<usize>]) -> Option<f64> {
    let k = assignments.iter().flatten().max()? + 1;
    if k < 2 {
        return None;
    }
    let dim = vectors.first()?.len();

    // Mean centroids, deliberately not normalized: scatter and separation
    // are measured against the true cluster means.
    let (centroids, counts) =
        super::dense_clustering::mean_centroids(vectors, assignments.iter().copied(), k, dim);

    let mut scatter = vec![0.0f64; k];
    for (v, a) in vectors.iter().zip(assignments.iter()) {
        if let Some(c) = a {
            scatter[*c] += f64::from(cosine_distance(v, &centroids[*c]));
        }
    }
    for (s, n) in scatter.iter_mut().zip(counts.iter()) {
        if *n > 0 {
            *s /= *n as f64;
        }
    }

    let live: Vec<usize> = (0..k).filter(|&c| counts[c] > 0).collect();
    if live.len() < 2 {
        return None;
    }
    let mut total = 0.0f64;
    for &i in &live {
        let mut worst = 0.0f64;
        for &j in &live {
            if i == j {
                continue;
            }
            let dist = f64::from(cosine_distance(&centroids[i], &centroids[j])).max(1e-12);
            worst = worst.max((scatter[i] + scatter[j]) / dist);
        }
        total += worst;
    }
    Some(total / live.len() as f64)
}

/// Mean pairwise NPMI coherence of each cluster's top terms, averaged over
/// clusters. Uses document-level term occurrence over the provided texts.
/// Range roughly [-1, 1]; higher = terms genuinely co-occur.
pub fn npmi_coherence(texts: &[String], cluster_terms: &[Vec<String>]) -> Option<f64> {
    let n_docs = texts.len();
    if n_docs == 0 {
        return None;
    }
    let lowered: Vec<String> = texts.iter().map(|t| t.to_lowercase()).collect();
    let doc_has = |term: &str| -> Vec<bool> { lowered.iter().map(|t| t.contains(term)).collect() };

    let mut cluster_scores = Vec::new();
    for terms in cluster_terms {
        let terms: Vec<&String> = terms.iter().take(6).collect();
        if terms.len() < 2 {
            continue;
        }
        let occurrence: Vec<Vec<bool>> = terms.iter().map(|t| doc_has(&t.to_lowercase())).collect();
        let mut pair_scores = Vec::new();
        for i in 0..terms.len() {
            for j in (i + 1)..terms.len() {
                let p_i = count_true(&occurrence[i]) as f64 / n_docs as f64;
                let p_j = count_true(&occurrence[j]) as f64 / n_docs as f64;
                let p_ij = occurrence[i]
                    .iter()
                    .zip(occurrence[j].iter())
                    .filter(|(a, b)| **a && **b)
                    .count() as f64
                    / n_docs as f64;
                if p_i <= 0.0 || p_j <= 0.0 {
                    continue;
                }
                let eps = 1e-12;
                let pmi = ((p_ij + eps) / (p_i * p_j)).ln();
                let npmi = pmi / -(p_ij + eps).ln();
                pair_scores.push(npmi);
            }
        }
        if !pair_scores.is_empty() {
            cluster_scores.push(pair_scores.iter().sum::<f64>() / pair_scores.len() as f64);
        }
    }
    if cluster_scores.is_empty() {
        None
    } else {
        Some(cluster_scores.iter().sum::<f64>() / cluster_scores.len() as f64)
    }
}

/// Adjusted Rand Index between a predicted assignment (None = unassigned,
/// excluded) and ground-truth labels. 1.0 = perfect recovery, ~0 = random.
pub fn adjusted_rand_index(predicted: &[Option<usize>], truth: &[usize]) -> Option<f64> {
    let pairs: Vec<(usize, usize)> = predicted
        .iter()
        .zip(truth.iter())
        .filter_map(|(p, t)| p.map(|p| (p, *t)))
        .collect();
    let n = pairs.len();
    if n < 2 {
        return None;
    }
    let mut contingency: HashMap<(usize, usize), u64> = HashMap::new();
    let mut row_sums: HashMap<usize, u64> = HashMap::new();
    let mut col_sums: HashMap<usize, u64> = HashMap::new();
    for (p, t) in &pairs {
        *contingency.entry((*p, *t)).or_insert(0) += 1;
        *row_sums.entry(*p).or_insert(0) += 1;
        *col_sums.entry(*t).or_insert(0) += 1;
    }
    let comb2 = |x: u64| -> f64 { (x as f64) * (x as f64 - 1.0) / 2.0 };
    let sum_ij: f64 = contingency.values().map(|&v| comb2(v)).sum();
    let sum_a: f64 = row_sums.values().map(|&v| comb2(v)).sum();
    let sum_b: f64 = col_sums.values().map(|&v| comb2(v)).sum();
    let total = comb2(n as u64);
    let expected = sum_a * sum_b / total;
    let max_index = 0.5 * (sum_a + sum_b);
    let denom = max_index - expected;
    if denom.abs() < 1e-12 {
        return Some(0.0);
    }
    Some((sum_ij - expected) / denom)
}

fn count_true(v: &[bool]) -> usize {
    v.iter().filter(|b| **b).count()
}

/// Cosine distance (1 − cosine similarity) for arbitrary dense vectors.
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    1.0 - super::similarity::dense_cosine_unnormalized(a, b)
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, clippy::redundant_clone)]
mod tests {
    use super::*;

    fn blobs() -> (Vec<Vec<f32>>, Vec<Option<usize>>) {
        let mut vectors = Vec::new();
        let mut assignments = Vec::new();
        for i in 0..20 {
            let j = (i % 5) as f32 * 0.01;
            vectors.push(vec![1.0, j, 0.0]);
            assignments.push(Some(0));
            vectors.push(vec![0.0, j, 1.0]);
            assignments.push(Some(1));
        }
        (vectors, assignments)
    }

    #[test]
    fn silhouette_high_for_separated_blobs() {
        let (v, a) = blobs();
        let s = silhouette(&v, &a).unwrap();
        assert!(s > 0.8, "s={s}");
    }

    #[test]
    fn silhouette_low_for_shuffled_labels() {
        let (v, mut a) = blobs();
        // Alternate labels randomly relative to geometry
        for (i, x) in a.iter_mut().enumerate() {
            *x = Some((i / 2) % 2);
        }
        let s = silhouette(&v, &a).unwrap();
        assert!(s < 0.3, "s={s}");
    }

    #[test]
    fn davies_bouldin_lower_for_separated() {
        let (v, a) = blobs();
        let good = davies_bouldin(&v, &a).unwrap();
        let mut shuffled = a.clone();
        for (i, x) in shuffled.iter_mut().enumerate() {
            *x = Some((i / 2) % 2);
        }
        let bad = davies_bouldin(&v, &shuffled).unwrap();
        assert!(good < bad, "good={good} bad={bad}");
    }

    #[test]
    fn ari_perfect_and_random() {
        let truth: Vec<usize> = (0..40).map(|i| i % 4).collect();
        let perfect: Vec<Option<usize>> = truth.iter().map(|t| Some(*t)).collect();
        assert!((adjusted_rand_index(&perfect, &truth).unwrap() - 1.0).abs() < 1e-9);
        // Permuted label names still count as perfect recovery
        let renamed: Vec<Option<usize>> = truth.iter().map(|t| Some((t + 1) % 4)).collect();
        assert!((adjusted_rand_index(&renamed, &truth).unwrap() - 1.0).abs() < 1e-9);
        let constant: Vec<Option<usize>> = truth.iter().map(|_| Some(0)).collect();
        assert!(adjusted_rand_index(&constant, &truth).unwrap().abs() < 0.05);
    }

    #[test]
    fn npmi_rewards_cooccurring_terms() {
        let texts: Vec<String> = (0..50)
            .map(|i| {
                if i % 2 == 0 {
                    "rust compiler borrow checker".to_string()
                } else {
                    "python pandas dataframe".to_string()
                }
            })
            .collect();
        let coherent =
            npmi_coherence(&texts, &[vec!["rust".to_string(), "compiler".to_string()]]).unwrap();
        let incoherent =
            npmi_coherence(&texts, &[vec!["rust".to_string(), "pandas".to_string()]]).unwrap();
        assert!(coherent > incoherent, "{coherent} vs {incoherent}");
    }

    #[test]
    fn cosine_distance_known_value() {
        // cos([3,4],[4,3]) = 24/25 = 0.96 → distance 0.04
        assert!((cosine_distance(&[3.0, 4.0], &[4.0, 3.0]) - 0.04).abs() < 1e-6);
        assert!((cosine_distance(&[1.0, 0.0], &[1.0, 0.0])).abs() < 1e-6);
        // Zero vector floors to similarity 0 → distance 1
        assert!((cosine_distance(&[0.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
    }
}

//! Dimensionality reduction for density clustering.
//!
//! HDBSCAN degrades in high dimensions (distances concentrate), so embeddings
//! are reduced to 5–20 principal components first. Hand-rolled PCA keeps the
//! dependency tree flat: Gram-matrix trick when n < d, power iteration with
//! deflation for the top components. Deterministic — no RNG.

/// A fitted PCA projection.
#[derive(Debug, Clone)]
pub struct Pca {
    /// Component vectors, one per output dim, each of input-dim length.
    pub components: Vec<Vec<f32>>,
    /// Input-space mean subtracted before projection.
    pub mean: Vec<f32>,
}

const POWER_ITERATIONS: usize = 60;
const CONVERGENCE_EPS: f32 = 1e-6;

impl Pca {
    /// Fit the top `n_components` principal directions.
    pub fn fit(vectors: &[Vec<f32>], n_components: usize) -> Option<Self> {
        let n = vectors.len();
        if n < 3 {
            return None;
        }
        let d = vectors[0].len();
        let k = n_components.min(n - 1).min(d);
        if k == 0 {
            return None;
        }

        let mut mean = vec![0.0f32; d];
        for v in vectors {
            for (m, x) in mean.iter_mut().zip(v.iter()) {
                *m += x;
            }
        }
        for m in &mut mean {
            *m /= n as f32;
        }
        let centered: Vec<Vec<f32>> = vectors
            .iter()
            .map(|v| v.iter().zip(mean.iter()).map(|(x, m)| x - m).collect())
            .collect();

        // Gram trick: eigenvectors of the n×n Gram matrix map back to
        // input-space components; much cheaper than d×d covariance when n < d.
        let use_gram = n < d;
        let mut components: Vec<Vec<f32>> = Vec::with_capacity(k);
        if use_gram {
            let mut gram = vec![vec![0.0f32; n]; n];
            for i in 0..n {
                for j in i..n {
                    let dot = dot(&centered[i], &centered[j]);
                    gram[i][j] = dot;
                    gram[j][i] = dot;
                }
            }
            let mut deflated = gram;
            for c in 0..k {
                let Some((eigvec, eigval)) = power_iteration(&deflated, c) else {
                    break;
                };
                if eigval <= 1e-9 {
                    break;
                }
                // Map Gram eigenvector u to input space: w = Σ u_i · x_i
                let mut w = vec![0.0f32; d];
                for (u_i, x) in eigvec.iter().zip(centered.iter()) {
                    for (w_j, x_j) in w.iter_mut().zip(x.iter()) {
                        *w_j += u_i * x_j;
                    }
                }
                normalize(&mut w);
                components.push(w);
                deflate(&mut deflated, &eigvec, eigval);
            }
        } else {
            let mut cov = vec![vec![0.0f32; d]; d];
            #[allow(clippy::needless_range_loop, clippy::indexing_slicing)]
            for x in &centered {
                for i in 0..d {
                    let xi = x[i];
                    if xi == 0.0 {
                        continue;
                    }
                    for j in i..d {
                        cov[i][j] = xi.mul_add(x[j], cov[i][j]);
                    }
                }
            }
            #[allow(clippy::indexing_slicing, clippy::needless_range_loop)]
            for i in 0..d {
                for j in 0..i {
                    cov[i][j] = cov[j][i];
                }
            }
            let mut deflated = cov;
            for c in 0..k {
                let Some((mut eigvec, eigval)) = power_iteration(&deflated, c) else {
                    break;
                };
                if eigval <= 1e-9 {
                    break;
                }
                normalize(&mut eigvec);
                deflate(&mut deflated, &eigvec, eigval);
                components.push(eigvec);
            }
        }

        if components.is_empty() {
            return None;
        }
        Some(Self { components, mean })
    }

    /// Project a vector into component space.
    pub fn transform(&self, v: &[f32]) -> Vec<f32> {
        let centered: Vec<f32> = v.iter().zip(self.mean.iter()).map(|(x, m)| x - m).collect();
        self.components.iter().map(|c| dot(c, &centered)).collect()
    }

    pub fn transform_batch(&self, vectors: &[Vec<f32>]) -> Vec<Vec<f32>> {
        vectors.iter().map(|v| self.transform(v)).collect()
    }
}

/// Dominant eigenpair of a symmetric matrix via power iteration.
/// Deterministic start vector varies with `salt` to escape unlucky
/// orthogonality with the dominant eigenvector.
fn power_iteration(matrix: &[Vec<f32>], salt: usize) -> Option<(Vec<f32>, f32)> {
    let n = matrix.len();
    if n == 0 {
        return None;
    }
    let mut v: Vec<f32> = (0..n)
        .map(|i| {
            // Deterministic pseudo-random start
            let x = (i * 2_654_435_761 + salt * 40_503 + 1) % 1000;
            (x as f32 / 1000.0) - 0.5
        })
        .collect();
    normalize(&mut v);

    let mut eigval = 0.0f32;
    for _ in 0..POWER_ITERATIONS {
        let mut next = vec![0.0f32; n];
        for (i, row) in matrix.iter().enumerate() {
            next[i] = dot(row, &v);
        }
        let norm = next.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm <= 1e-12 {
            return None;
        }
        for x in &mut next {
            *x /= norm;
        }
        let delta: f32 = next
            .iter()
            .zip(v.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        v = next;
        eigval = norm;
        if delta < CONVERGENCE_EPS {
            break;
        }
    }
    Some((v, eigval))
}

/// Remove an eigenpair's contribution: M ← M − λ·v·vᵀ
fn deflate(matrix: &mut [Vec<f32>], eigvec: &[f32], eigval: f32) {
    for (i, row) in matrix.iter_mut().enumerate() {
        for (j, m) in row.iter_mut().enumerate() {
            *m = (eigval * eigvec[i]).mul_add(-eigvec[j], *m);
        }
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for x in v {
            *x /= norm;
        }
    }
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss)]
mod tests {
    use super::*;

    #[test]
    fn recovers_dominant_direction() {
        // Points spread along the (1, 1, 0) direction with small noise
        let vectors: Vec<Vec<f32>> = (0..50)
            .map(|i| {
                let t = (i as f32) - 25.0;
                let noise = ((i * 7) % 5) as f32 * 0.01;
                vec![t + noise, t - noise, noise]
            })
            .collect();
        let pca = Pca::fit(&vectors, 1).unwrap();
        let c = &pca.components[0];
        // First component should point along (±1/√2, ±1/√2, ~0)
        assert!(c[2].abs() < 0.05, "third axis is noise: {c:?}");
        assert!(
            (c[0].abs() - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.05,
            "{c:?}"
        );
    }

    #[test]
    fn projection_separates_planted_clusters() {
        // Two blobs in 20-dim space, separated on a diagonal
        let mut vectors = Vec::new();
        for i in 0..30 {
            let jitter = ((i * 13) % 7) as f32 * 0.02;
            let mut a = vec![jitter; 20];
            a[0] += 5.0;
            vectors.push(a);
            let mut b = vec![-jitter; 20];
            b[0] -= 5.0;
            vectors.push(b);
        }
        let pca = Pca::fit(&vectors, 2).unwrap();
        let projected = pca.transform_batch(&vectors);
        // Blob A projections and blob B projections must separate on dim 0
        let a_mean: f32 = projected.iter().step_by(2).map(|p| p[0]).sum::<f32>() / 30.0;
        let b_mean: f32 = projected
            .iter()
            .skip(1)
            .step_by(2)
            .map(|p| p[0])
            .sum::<f32>()
            / 30.0;
        assert!((a_mean - b_mean).abs() > 5.0, "a={a_mean} b={b_mean}");
    }

    #[test]
    fn deterministic() {
        let vectors: Vec<Vec<f32>> = (0..40)
            .map(|i| vec![(i % 7) as f32, (i % 3) as f32, (i % 11) as f32])
            .collect();
        let a = Pca::fit(&vectors, 2).unwrap();
        let b = Pca::fit(&vectors, 2).unwrap();
        assert_eq!(a.components, b.components);
    }
}

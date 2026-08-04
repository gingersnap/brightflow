//! Cosine similarity over sparse and dense vectors.
//!
//! Paired entry points on purpose: `cosine`/`dense_cosine` assume
//! L2-normalized input and are a bare dot product, which is the common case
//! after a TF-IDF transform or embedding normalization;
//! `cosine_unnormalized`/`dense_cosine_unnormalized` pay for the norms when
//! that assumption does not hold.
//!
//! The dense kernels use `mul_add` deliberately: one rounding per term keeps
//! results reproducible across the fingerprinted artifacts that embed these
//! numbers, so every caller must go through the same kernel rather than
//! hand-rolling `x * y` sums with different rounding.

use super::sparse::SparseVec;

/// Dense dot product (fused multiply-add per term).
#[inline]
pub fn dot_dense(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .fold(0.0_f32, |acc, (x, y)| x.mul_add(*y, acc))
}

/// Cosine similarity for pre-normalized dense vectors (bare dot product).
#[inline]
pub fn dense_cosine(a: &[f32], b: &[f32]) -> f32 {
    dot_dense(a, b)
}

/// Cosine similarity for arbitrary (unnormalized) dense vectors.
///
/// The `1e-12` floor keeps a zero vector at similarity 0 instead of NaN.
pub fn dense_cosine_unnormalized(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot = x.mul_add(*y, dot);
        na = x.mul_add(*x, na);
        nb = y.mul_add(*y, nb);
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    dot / denom
}

/// Cosine similarity for pre-normalized vectors (fast path).
///
/// If both vectors are L2-normalized (the default after TF-IDF transform),
/// cosine similarity equals the dot product.
pub fn cosine(a: &SparseVec, b: &SparseVec) -> f32 {
    a.dot(b)
}

/// Cosine similarity for arbitrary (unnormalized) vectors.
///
/// Computes `dot(a, b) / (||a|| * ||b||)`. Returns 0.0 if either vector
/// has zero norm.
pub fn cosine_unnormalized(a: &SparseVec, b: &SparseVec) -> f32 {
    let dot = a.dot(b);
    let norm_a = a.l2_norm();
    let norm_b = b.l2_norm();
    let denom = norm_a * norm_b;
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact expected values")]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical_normalized() {
        let mut a = SparseVec::new(vec![0, 1, 2], vec![1.0, 2.0, 3.0], 0).unwrap();
        a.normalize();
        let sim = cosine(&a, &a);
        assert!((sim - 1.0).abs() < 1e-5, "identical normalized: {sim}");
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = SparseVec::new(vec![0, 1], vec![1.0, 0.0], 0).unwrap();
        let b = SparseVec::new(vec![2, 3], vec![0.0, 1.0], 0).unwrap();
        assert_eq!(cosine(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_unnormalized_known() {
        // a = (3, 4), b = (4, 3)
        // dot = 12 + 12 = 24
        // ||a|| = 5, ||b|| = 5
        // cosine = 24/25 = 0.96
        let a = SparseVec::new(vec![0, 1], vec![3.0, 4.0], 0).unwrap();
        let b = SparseVec::new(vec![0, 1], vec![4.0, 3.0], 0).unwrap();
        let sim = cosine_unnormalized(&a, &b);
        assert!((sim - 0.96).abs() < 1e-5, "expected 0.96, got {sim}");
    }

    #[test]
    fn test_cosine_empty_vector() {
        let a = SparseVec::new(vec![0, 1], vec![1.0, 2.0], 0).unwrap();
        let b = SparseVec::empty(0);
        assert_eq!(cosine(&a, &b), 0.0);
        assert_eq!(cosine_unnormalized(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_both_empty() {
        let a = SparseVec::empty(0);
        let b = SparseVec::empty(0);
        assert_eq!(cosine_unnormalized(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_normalized_matches_unnormalized() {
        let a_raw = SparseVec::new(vec![0, 1, 5], vec![1.0, 3.0, 2.0], 0).unwrap();
        let b_raw = SparseVec::new(vec![0, 1, 7], vec![2.0, 1.0, 4.0], 0).unwrap();

        let sim_unnorm = cosine_unnormalized(&a_raw, &b_raw);
        let sim_norm = cosine(&a_raw.normalized(), &b_raw.normalized());

        assert!(
            (sim_unnorm - sim_norm).abs() < 1e-5,
            "unnorm={sim_unnorm}, norm={sim_norm}"
        );
    }

    #[test]
    fn dot_dense_known_value() {
        assert!((dot_dense(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]) - 32.0).abs() < 1e-6);
        assert_eq!(dot_dense(&[], &[]), 0.0);
    }

    #[test]
    fn dense_cosine_unnormalized_known_value() {
        // (3,4)·(4,3) = 24, norms 5·5 → 0.96
        assert!((dense_cosine_unnormalized(&[3.0, 4.0], &[4.0, 3.0]) - 0.96).abs() < 1e-6);
        // zero vector floors to 0, not NaN
        assert_eq!(dense_cosine_unnormalized(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }
}

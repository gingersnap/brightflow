use super::sparse::SparseVec;

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
#[allow(clippy::float_cmp)]
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
}

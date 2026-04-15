use crate::error::{Result, SubtextError};

/// A sparse vector with sorted `u32` indices and `f32` values.
///
/// The canonical substrate for TF-IDF vectors and cosine similarity.
/// Indices are always sorted in ascending order with no duplicates.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SparseVec {
    indices: Vec<u32>,
    values: Vec<f32>,
    generation: u64,
}

impl SparseVec {
    /// Create a new sparse vector from pre-sorted indices and values.
    ///
    /// Returns an error if indices and values have different lengths,
    /// or if indices are not strictly ascending.
    pub fn new(indices: Vec<u32>, values: Vec<f32>, generation: u64) -> Result<Self> {
        if indices.len() != values.len() {
            return Err(SubtextError::LengthMismatch {
                indices_len: indices.len(),
                values_len: values.len(),
            });
        }
        for window in indices.windows(2) {
            if window[0] >= window[1] {
                return Err(SubtextError::IndicesNotSorted {
                    position: 1,
                    prev: window[0],
                    current: window[1],
                });
            }
        }
        Ok(Self {
            indices,
            values,
            generation,
        })
    }

    /// Create a sparse vector from unsorted `(index, value)` pairs.
    ///
    /// Sorts by index and sums values at duplicate indices.
    pub fn from_unsorted(mut pairs: Vec<(u32, f32)>, generation: u64) -> Self {
        if pairs.is_empty() {
            return Self::empty(generation);
        }
        pairs.sort_by_key(|&(idx, _)| idx);

        let mut indices = Vec::with_capacity(pairs.len());
        let mut values = Vec::with_capacity(pairs.len());

        let (mut cur_idx, mut cur_val) = pairs[0];
        for &(idx, val) in &pairs[1..] {
            if idx == cur_idx {
                cur_val += val;
            } else {
                if cur_val != 0.0 {
                    indices.push(cur_idx);
                    values.push(cur_val);
                }
                cur_idx = idx;
                cur_val = val;
            }
        }
        if cur_val != 0.0 {
            indices.push(cur_idx);
            values.push(cur_val);
        }

        Self {
            indices,
            values,
            generation,
        }
    }

    /// Create an empty sparse vector.
    pub fn empty(generation: u64) -> Self {
        Self {
            indices: Vec::new(),
            values: Vec::new(),
            generation,
        }
    }

    /// Whether this vector has no non-zero entries.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Number of non-zero entries.
    pub fn nnz(&self) -> usize {
        self.indices.len()
    }

    /// The generation this vector was computed from.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Read-only access to indices.
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Read-only access to values.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Iterate over `(index, value)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.indices
            .iter()
            .copied()
            .zip(self.values.iter().copied())
    }

    /// Dot product with another sparse vector using sorted-merge scan.
    pub fn dot(&self, other: &Self) -> f32 {
        let mut result = 0.0_f32;
        let mut i = 0;
        let mut j = 0;
        while i < self.indices.len() && j < other.indices.len() {
            match self.indices[i].cmp(&other.indices[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    result = self.values[i].mul_add(other.values[j], result);
                    i += 1;
                    j += 1;
                },
            }
        }
        result
    }

    /// L2 (Euclidean) norm.
    pub fn l2_norm(&self) -> f32 {
        self.values
            .iter()
            .fold(0.0_f32, |acc, &v| v.mul_add(v, acc))
            .sqrt()
    }

    /// Normalize this vector to unit L2 norm in place.
    pub fn normalize(&mut self) {
        let norm = self.l2_norm();
        if norm > 0.0 {
            for v in &mut self.values {
                *v /= norm;
            }
        }
    }

    /// Return a new vector normalized to unit L2 norm.
    pub fn normalized(&self) -> Self {
        let mut clone = self.clone();
        clone.normalize();
        clone
    }

    /// Element-wise add another vector into this one.
    ///
    /// Used for accumulating centroid sums. Merges sorted index arrays
    /// and sums values at matching indices.
    pub fn add_assign(&mut self, other: &Self) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            self.indices.clone_from(&other.indices);
            self.values.clone_from(&other.values);
            return;
        }

        let mut new_indices = Vec::with_capacity(self.indices.len() + other.indices.len());
        let mut new_values = Vec::with_capacity(self.values.len() + other.values.len());

        let mut i = 0;
        let mut j = 0;
        while i < self.indices.len() && j < other.indices.len() {
            match self.indices[i].cmp(&other.indices[j]) {
                std::cmp::Ordering::Less => {
                    new_indices.push(self.indices[i]);
                    new_values.push(self.values[i]);
                    i += 1;
                },
                std::cmp::Ordering::Greater => {
                    new_indices.push(other.indices[j]);
                    new_values.push(other.values[j]);
                    j += 1;
                },
                std::cmp::Ordering::Equal => {
                    let sum = self.values[i] + other.values[j];
                    if sum != 0.0 {
                        new_indices.push(self.indices[i]);
                        new_values.push(sum);
                    }
                    i += 1;
                    j += 1;
                },
            }
        }
        while i < self.indices.len() {
            new_indices.push(self.indices[i]);
            new_values.push(self.values[i]);
            i += 1;
        }
        while j < other.indices.len() {
            new_indices.push(other.indices[j]);
            new_values.push(other.values[j]);
            j += 1;
        }

        self.indices = new_indices;
        self.values = new_values;
    }

    /// Scalar division in place. Used for computing centroid averages.
    pub fn scale(&mut self, scalar: f32) {
        if scalar == 0.0 {
            return;
        }
        for v in &mut self.values {
            *v /= scalar;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_new_valid() {
        let v = SparseVec::new(vec![0, 2, 5], vec![1.0, 2.0, 3.0], 0).unwrap();
        assert_eq!(v.nnz(), 3);
        assert!(!v.is_empty());
    }

    #[test]
    fn test_new_length_mismatch() {
        let result = SparseVec::new(vec![0, 1], vec![1.0], 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_unsorted_indices() {
        let result = SparseVec::new(vec![5, 2, 0], vec![1.0, 2.0, 3.0], 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_duplicate_indices() {
        let result = SparseVec::new(vec![0, 0, 1], vec![1.0, 2.0, 3.0], 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty() {
        let v = SparseVec::empty(0);
        assert!(v.is_empty());
        assert_eq!(v.nnz(), 0);
        assert_eq!(v.l2_norm(), 0.0);
    }

    #[test]
    fn test_from_unsorted() {
        let v = SparseVec::from_unsorted(vec![(5, 3.0), (0, 1.0), (2, 2.0)], 0);
        assert_eq!(v.indices(), &[0, 2, 5]);
        assert_eq!(v.values(), &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_from_unsorted_dedup() {
        let v = SparseVec::from_unsorted(vec![(1, 1.0), (1, 2.0), (3, 4.0)], 0);
        assert_eq!(v.indices(), &[1, 3]);
        assert_eq!(v.values(), &[3.0, 4.0]);
    }

    #[test]
    fn test_dot_overlapping() {
        let a = SparseVec::new(vec![0, 2, 5], vec![1.0, 2.0, 3.0], 0).unwrap();
        let b = SparseVec::new(vec![2, 5, 7], vec![4.0, 5.0, 6.0], 0).unwrap();
        // dot = 2*4 + 3*5 = 8 + 15 = 23
        let d = a.dot(&b);
        assert!((d - 23.0).abs() < 1e-6);
    }

    #[test]
    fn test_dot_disjoint() {
        let a = SparseVec::new(vec![0, 1], vec![1.0, 2.0], 0).unwrap();
        let b = SparseVec::new(vec![3, 4], vec![3.0, 4.0], 0).unwrap();
        assert_eq!(a.dot(&b), 0.0);
    }

    #[test]
    fn test_dot_empty() {
        let a = SparseVec::new(vec![0, 1], vec![1.0, 2.0], 0).unwrap();
        let b = SparseVec::empty(0);
        assert_eq!(a.dot(&b), 0.0);
    }

    #[test]
    fn test_l2_norm() {
        // sqrt(3^2 + 4^2) = 5
        let v = SparseVec::new(vec![0, 1], vec![3.0, 4.0], 0).unwrap();
        assert!((v.l2_norm() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize() {
        let mut v = SparseVec::new(vec![0, 1], vec![3.0, 4.0], 0).unwrap();
        v.normalize();
        assert!((v.l2_norm() - 1.0).abs() < 1e-6);
        assert!((v.values()[0] - 0.6).abs() < 1e-6);
        assert!((v.values()[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_idempotent() {
        let mut v = SparseVec::new(vec![0, 1], vec![3.0, 4.0], 0).unwrap();
        v.normalize();
        let first = v.values().to_vec();
        v.normalize();
        for (a, b) in v.values().iter().zip(first.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_normalize_empty() {
        let mut v = SparseVec::empty(0);
        v.normalize(); // should not panic
        assert!(v.is_empty());
    }

    #[test]
    fn test_add_assign() {
        let mut a = SparseVec::new(vec![0, 2, 5], vec![1.0, 2.0, 3.0], 0).unwrap();
        let b = SparseVec::new(vec![1, 2, 7], vec![4.0, 5.0, 6.0], 0).unwrap();
        a.add_assign(&b);
        assert_eq!(a.indices(), &[0, 1, 2, 5, 7]);
        assert_eq!(a.values(), &[1.0, 4.0, 7.0, 3.0, 6.0]);
    }

    #[test]
    fn test_add_assign_empty() {
        let mut a = SparseVec::new(vec![0, 1], vec![1.0, 2.0], 0).unwrap();
        let b = SparseVec::empty(0);
        a.add_assign(&b);
        assert_eq!(a.indices(), &[0, 1]);
    }

    #[test]
    fn test_scale() {
        let mut v = SparseVec::new(vec![0, 1], vec![4.0, 6.0], 0).unwrap();
        v.scale(2.0);
        assert!((v.values()[0] - 2.0).abs() < 1e-6);
        assert!((v.values()[1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_iter() {
        let v = SparseVec::new(vec![0, 5, 10], vec![1.0, 2.0, 3.0], 0).unwrap();
        let pairs: Vec<_> = v.iter().collect();
        assert_eq!(pairs, vec![(0, 1.0), (5, 2.0), (10, 3.0)]);
    }

    #[test]
    fn test_generation() {
        let v = SparseVec::empty(42);
        assert_eq!(v.generation(), 42);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_serde_roundtrip() {
        let v = SparseVec::new(vec![0, 2, 5], vec![1.0, 2.0, 3.0], 7).unwrap();
        let json = serde_json::to_string(&v).unwrap();
        let v2: SparseVec = serde_json::from_str(&json).unwrap();
        assert_eq!(v, v2);
    }
}

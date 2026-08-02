use crate::nlp::error::{Result, SubtextError};
use crate::nlp::SparseVec;

/// Serialize a `SparseVec` to bytes for storage in a Polars Binary column.
pub fn sparse_vec_to_bytes(vec: &SparseVec) -> Result<Vec<u8>> {
    bincode::serialize(vec).map_err(|e| SubtextError::Serialization(e.to_string()))
}

/// Deserialize a `SparseVec` from bytes stored in a Polars Binary column.
pub fn sparse_vec_from_bytes(bytes: &[u8]) -> Result<SparseVec> {
    bincode::deserialize(bytes).map_err(|e| SubtextError::Serialization(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let vec = SparseVec::new(vec![0, 3, 7], vec![1.0, 2.5, 0.3], 5).unwrap();
        let bytes = sparse_vec_to_bytes(&vec).unwrap();
        let vec2 = sparse_vec_from_bytes(&bytes).unwrap();
        assert_eq!(vec, vec2);
    }

    #[test]
    fn test_roundtrip_empty() {
        let vec = SparseVec::empty(0);
        let bytes = sparse_vec_to_bytes(&vec).unwrap();
        let vec2 = sparse_vec_from_bytes(&bytes).unwrap();
        assert_eq!(vec, vec2);
    }

    #[test]
    fn test_invalid_bytes() {
        let result = sparse_vec_from_bytes(&[0xFF, 0xFF]);
        assert!(result.is_err());
    }
}

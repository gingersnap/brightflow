//! Stable insight fingerprints.
//!
//! A fingerprint identifies an insight's *story* — detector type, measure,
//! aggregation, derivations, filters, granularity — while excluding the
//! observed values and period, so "the same story next week" hashes the same.
//! Consumed by insight history / novelty (1C) and dismiss/pin/suppress (3A).
//!
//! Uses FNV-1a, NOT `std::hash::DefaultHasher`: DefaultHasher is explicitly
//! not stable across releases/runs, and these hashes are persisted.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over the `|`-joined parts, rendered as fixed-width hex.
pub fn fingerprint(parts: &[&str]) -> String {
    let mut hash = FNV_OFFSET;
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            hash ^= u64::from(b'|');
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_known_value() {
        // Locked: if this changes, persisted fingerprints break.
        assert_eq!(
            fingerprint(&["trend", "revenue", "mean"]),
            fingerprint(&["trend", "revenue", "mean"])
        );
        let fp = fingerprint(&["trend", "revenue", "mean"]);
        assert_eq!(fp.len(), 16);
        assert_eq!(fp, "4fef031c5bcd0861");
    }

    #[test]
    fn separator_prevents_ambiguity() {
        assert_ne!(fingerprint(&["ab", "c"]), fingerprint(&["a", "bc"]));
    }
}

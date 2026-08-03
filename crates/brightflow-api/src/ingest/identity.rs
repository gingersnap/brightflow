//! Privacy-preserving visitor identity.
//!
//! A visitor id is a truncated BLAKE3 hash of (salt, source, IP, user-agent).
//! IP and user-agent are inputs only — this returns a hash and retains nothing,
//! so identity can be derived without either value outliving the request.
//!
//! The id is stable for a given salt and changes whenever the salt does. Callers
//! are expected to rotate the salt daily, which is what bounds re-identification
//! to a single day: same-day sessionization stays possible, linking a visitor
//! across days stops being possible by construction rather than by policy.
//! Passing a fixed salt would silently defeat that.

/// Compute a privacy-preserving visitor ID.
///
/// Hash rotates daily with the salt, so the same physical visitor gets a
/// different `visitor_id` each day. IP and User-Agent are never stored.
#[must_use]
pub fn compute_visitor_id(salt: &str, source_id: &str, ip: &str, ua: &str) -> String {
    let input = format!("{salt}{source_id}{ip}{ua}");
    let hash = blake3::hash(input.as_bytes());
    // Truncate to 16 hex chars (64 bits) — sufficient for anonymous identity
    hash.to_hex()[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_hash() {
        let a = compute_visitor_id("salt1", "src1", "1.2.3.4", "Mozilla/5.0");
        let b = compute_visitor_id("salt1", "src1", "1.2.3.4", "Mozilla/5.0");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn different_salt_different_id() {
        let a = compute_visitor_id("salt1", "src1", "1.2.3.4", "Mozilla/5.0");
        let b = compute_visitor_id("salt2", "src1", "1.2.3.4", "Mozilla/5.0");
        assert_ne!(a, b);
    }

    #[test]
    fn different_ip_different_id() {
        let a = compute_visitor_id("salt1", "src1", "1.2.3.4", "Mozilla/5.0");
        let b = compute_visitor_id("salt1", "src1", "5.6.7.8", "Mozilla/5.0");
        assert_ne!(a, b);
    }
}

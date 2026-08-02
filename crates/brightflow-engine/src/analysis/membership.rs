//! Membership change: detects new and disappeared dimension values between
//! two periods. Triggers when at least one value is added or removed.

use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct MembershipChangeResult {
    pub segment_column: String,
    pub previous_period: String,
    pub current_period: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Membership size in the previous period (churn-null exposure)
    pub prev_size: usize,
    /// Membership size in the current period
    pub curr_size: usize,
}

pub fn detect_membership_change(
    segment_column: &str,
    segment_values: &[String],
    period_labels: &[Option<String>],
    previous_period: &str,
    current_period: &str,
) -> Option<MembershipChangeResult> {
    if segment_values.len() != period_labels.len() {
        return None;
    }
    let mut prev: HashSet<&str> = HashSet::new();
    let mut curr: HashSet<&str> = HashSet::new();
    for (val, p) in segment_values.iter().zip(period_labels.iter()) {
        if val.is_empty() {
            continue;
        }
        match p {
            Some(s) if s == previous_period => {
                prev.insert(val.as_str());
            },
            Some(s) if s == current_period => {
                curr.insert(val.as_str());
            },
            _ => {},
        }
    }
    if prev.is_empty() || curr.is_empty() {
        return None;
    }
    let added: Vec<String> = curr.difference(&prev).map(|s| (*s).to_string()).collect();
    let removed: Vec<String> = prev.difference(&curr).map(|s| (*s).to_string()).collect();
    if added.is_empty() && removed.is_empty() {
        return None;
    }
    let mut added_sorted = added;
    added_sorted.sort();
    let mut removed_sorted = removed;
    removed_sorted.sort();
    Some(MembershipChangeResult {
        segment_column: segment_column.to_string(),
        previous_period: previous_period.to_string(),
        current_period: current_period.to_string(),
        added: added_sorted,
        removed: removed_sorted,
        prev_size: prev.len(),
        curr_size: curr.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_added_and_removed() {
        let segs = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "b".to_string(),
            "d".to_string(),
        ];
        let periods = vec![
            Some("p1".to_string()),
            Some("p1".to_string()),
            Some("p1".to_string()),
            Some("p2".to_string()),
            Some("p2".to_string()),
        ];
        let r = detect_membership_change("region", &segs, &periods, "p1", "p2").unwrap();
        assert_eq!(r.added, vec!["d"]);
        assert_eq!(r.removed.len(), 2); // a, c
    }

    #[test]
    fn no_change_returns_none() {
        let segs = vec!["a".to_string(), "a".to_string()];
        let periods = vec![Some("p1".to_string()), Some("p2".to_string())];
        assert!(detect_membership_change("c", &segs, &periods, "p1", "p2").is_none());
    }
}

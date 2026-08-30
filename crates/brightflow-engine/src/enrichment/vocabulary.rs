//! Vocabulary kinds, caps and the reserved `other` value.
//!
//! Every closed list the LLM resolves against lives in one table; this module
//! is the pure half of that: which kinds exist, how they nest, and how many
//! entries a level may hold. The cap is a *review* limit, not an accuracy
//! threshold — a 10-item list is diffable by a human in two minutes, and that
//! review is what keeps the trend line honest. Over the cap the answer is
//! grouping (areas × components), never deletion. The hard backstop is an
//! engineering limit (prompt length, chart readability); the literature shows
//! continuous degradation with label count, not a cliff, so do not quote it
//! as one.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Entries a human is expected to review as a diff: induced kinds.
pub const INDUCED_CAP: usize = 10;
/// Entries per level for imported lists (products, competitors).
pub const IMPORTED_CAP: usize = 20;
/// Absolute ceiling per level, any kind.
pub const HARD_BACKSTOP: usize = 50;
/// Reserved value present at every induced level. Never a stored row: the
/// prompt offers it, the validator accepts it, and it counts toward the
/// other-rate health signal rather than toward the cap.
pub const OTHER: &str = "other";

/// Which list an entry belongs to. Serialized in snake_case to match the
/// `taxonomy_categories.kind` CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabKind {
    Category,
    Subcategory,
    FeedbackCategory,
    Product,
    Competitor,
}

impl VocabKind {
    pub const ALL: [Self; 5] = [
        Self::Category,
        Self::Subcategory,
        Self::FeedbackCategory,
        Self::Product,
        Self::Competitor,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Subcategory => "subcategory",
            Self::FeedbackCategory => "feedback_category",
            Self::Product => "product",
            Self::Competitor => "competitor",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == raw)
    }

    /// Induced from summaries by an agent run (10/level) vs imported from a
    /// customer list (20/level).
    pub fn is_induced(self) -> bool {
        matches!(
            self,
            Self::Category | Self::Subcategory | Self::FeedbackCategory
        )
    }

    pub fn cap(self) -> usize {
        if self.is_induced() {
            INDUCED_CAP
        } else {
            IMPORTED_CAP
        }
    }

    /// The kind a non-root entry's parent must have. `None` = this kind has
    /// no hierarchy and every entry is a root. Products nest under products
    /// (area → component).
    pub fn parent_kind(self) -> Option<Self> {
        match self {
            Self::Subcategory => Some(Self::Category),
            Self::Product => Some(Self::Product),
            Self::Category | Self::FeedbackCategory | Self::Competitor => None,
        }
    }

    /// Whether an entry of this kind may sit at the root (parent 0).
    pub fn allows_root(self) -> bool {
        self != Self::Subcategory
    }

    /// Whether an entry of this kind may have children.
    pub fn allows_children(self) -> bool {
        matches!(self, Self::Category | Self::Product)
    }
}

impl fmt::Display for VocabKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an entry may not be added at a level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapError {
    /// The level holds `count` entries against a cap of `cap`. Group, do not
    /// delete.
    AtCap {
        kind: VocabKind,
        count: usize,
        cap: usize,
    },
    /// The name is the reserved value.
    Reserved,
    /// Empty after trimming.
    EmptyName,
}

impl fmt::Display for CapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AtCap { kind, count, cap } => write!(
                f,
                "{kind} already has {count} entries at this level (cap {cap}) — group \
                 existing entries rather than adding another"
            ),
            Self::Reserved => write!(f, "'{OTHER}' is reserved and implicit at every level"),
            Self::EmptyName => write!(f, "name cannot be empty"),
        }
    }
}

impl std::error::Error for CapError {}

/// Whether `name` is the reserved value (case-insensitive, trimmed).
pub fn is_other(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case(OTHER)
}

/// Check that one more entry named `name` may join a level.
///
/// `existing` are the level's current names. Names already present do not
/// count twice — an upsert of an existing name is always allowed. The
/// reserved value is never stored and never counts.
pub fn check_cap<'a>(
    kind: VocabKind,
    name: &str,
    existing: impl IntoIterator<Item = &'a str>,
) -> Result<(), CapError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CapError::EmptyName);
    }
    if is_other(name) {
        return Err(CapError::Reserved);
    }
    let mut count = 0_usize;
    for n in existing {
        if is_other(n) {
            continue;
        }
        if n.eq_ignore_ascii_case(name) {
            return Ok(());
        }
        count += 1;
    }
    let cap = kind.cap().min(HARD_BACKSTOP);
    if count >= cap {
        return Err(CapError::AtCap { kind, count, cap });
    }
    Ok(())
}

/// Per-level health at steady state: the numbers that say whether a
/// vocabulary is wrong, once there is data. Count is only a cold-start signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelHealth {
    pub kind: VocabKind,
    /// Stored entries at this level (the reserved value excluded).
    pub entries: usize,
    pub cap: usize,
    /// Rows classified at this level, including `other`.
    pub rows: usize,
    /// Share of rows landing in `other`. Above ~0.15 the vocabulary is wrong.
    pub other_rate: f64,
    /// Largest and smallest non-`other` share.
    pub max_share: f64,
    pub min_share: f64,
    /// Entries outside the 2 %–40 % balance band, with their share.
    pub unbalanced: Vec<(String, f64)>,
}

/// Other-rate threshold above which the vocabulary needs work.
pub const OTHER_RATE_WARN: f64 = 0.15;
/// Balance band: nothing above 40 % or below 2 %.
pub const BALANCE_MAX: f64 = 0.40;
pub const BALANCE_MIN: f64 = 0.02;

/// Compute a level's health from `(value, row_count)` pairs.
///
/// `other` is matched case-insensitively and reported separately; entries
/// with zero rows are still unbalanced (below the floor) — a
/// defined-but-unused entry is exactly the finding a curator needs.
pub fn health(kind: VocabKind, counts: &[(String, usize)]) -> LevelHealth {
    let rows: usize = counts.iter().map(|(_, n)| n).sum();
    let other_rows: usize = counts
        .iter()
        .filter(|(v, _)| is_other(v))
        .map(|(_, n)| n)
        .sum();
    let denom = if rows == 0 { 1.0 } else { rows as f64 };
    let mut max_share = 0.0_f64;
    let mut min_share: f64 = if rows == 0 { 0.0 } else { 1.0 };
    let mut unbalanced = Vec::new();
    let mut entries = 0_usize;
    for (value, n) in counts {
        if is_other(value) {
            continue;
        }
        entries += 1;
        let share = *n as f64 / denom;
        max_share = max_share.max(share);
        min_share = min_share.min(share);
        if rows > 0 && !(BALANCE_MIN..=BALANCE_MAX).contains(&share) {
            unbalanced.push((value.clone(), share));
        }
    }
    LevelHealth {
        kind,
        entries,
        cap: kind.cap(),
        rows,
        other_rate: other_rows as f64 / denom,
        max_share,
        min_share,
        unbalanced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("entry {i}")).collect()
    }

    #[test]
    fn induced_kinds_cap_at_ten_and_imported_at_twenty() {
        let nine = names(9);
        assert!(check_cap(VocabKind::Category, "new", nine.iter().map(String::as_str)).is_ok());
        let ten = names(10);
        assert_eq!(
            check_cap(VocabKind::Category, "new", ten.iter().map(String::as_str)),
            Err(CapError::AtCap {
                kind: VocabKind::Category,
                count: 10,
                cap: 10
            })
        );
        let nineteen = names(19);
        assert!(check_cap(
            VocabKind::Product,
            "new",
            nineteen.iter().map(String::as_str)
        )
        .is_ok());
        let twenty = names(20);
        assert!(check_cap(
            VocabKind::Competitor,
            "new",
            twenty.iter().map(String::as_str)
        )
        .is_err());
    }

    #[test]
    fn hard_backstop_bounds_every_kind() {
        for kind in VocabKind::ALL {
            assert!(kind.cap() <= HARD_BACKSTOP);
        }
    }

    #[test]
    fn other_is_reserved_and_never_counts() {
        assert_eq!(
            check_cap(VocabKind::Category, "Other", std::iter::empty()),
            Err(CapError::Reserved)
        );
        // Ten real entries plus a stray "other" row: still at cap, not over.
        let mut existing = names(10);
        existing.push("other".to_string());
        let err = check_cap(
            VocabKind::Category,
            "new",
            existing.iter().map(String::as_str),
        )
        .unwrap_err();
        assert!(matches!(err, CapError::AtCap { count: 10, .. }));
    }

    #[test]
    fn re_defining_an_existing_name_is_allowed_at_cap() {
        let ten = names(10);
        assert!(check_cap(
            VocabKind::Category,
            "Entry 3",
            ten.iter().map(String::as_str)
        )
        .is_ok());
        assert_eq!(
            check_cap(VocabKind::Category, "  ", std::iter::empty()),
            Err(CapError::EmptyName)
        );
    }

    #[test]
    fn kinds_round_trip_and_nest_as_designed() {
        for kind in VocabKind::ALL {
            assert_eq!(VocabKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(VocabKind::parse("nope"), None);
        assert_eq!(
            VocabKind::Subcategory.parent_kind(),
            Some(VocabKind::Category)
        );
        assert_eq!(VocabKind::Product.parent_kind(), Some(VocabKind::Product));
        assert!(!VocabKind::Subcategory.allows_root());
        assert!(VocabKind::Product.allows_root());
        assert!(!VocabKind::FeedbackCategory.allows_children());
    }

    #[test]
    fn health_reports_other_rate_and_balance_band() {
        let counts = vec![
            ("billing".to_string(), 50),
            ("login".to_string(), 30),
            ("rare".to_string(), 1),
            ("Other".to_string(), 19),
        ];
        let h = health(VocabKind::Category, &counts);
        assert_eq!(h.rows, 100);
        assert_eq!(h.entries, 3);
        assert!((h.other_rate - 0.19).abs() < 1e-9);
        assert!((h.max_share - 0.5).abs() < 1e-9);
        assert!((h.min_share - 0.01).abs() < 1e-9);
        let flagged: Vec<&str> = h.unbalanced.iter().map(|(v, _)| v.as_str()).collect();
        assert_eq!(flagged, vec!["billing", "rare"]);
    }

    #[test]
    fn health_on_empty_data_is_all_zero() {
        let h = health(VocabKind::Subcategory, &[]);
        assert_eq!(h.rows, 0);
        assert!(h.other_rate.abs() < f64::EPSILON);
        assert!(h.unbalanced.is_empty());
    }
}

//! Insight-history novelty: decay scores of stories the user has already seen.
//!
//! The engine stays DB-free — callers load prior exposure from wherever they
//! store it, convert it to [`HistoryEntry`] keyed by fingerprint, and hand the
//! map in; `apply_novelty` mutates the tree's score breakdowns before
//! selection.

use std::collections::HashMap;

use crate::analysis::scoring;
use crate::analysis::tree::{AnalysisNode, AnalysisTree, AnalysisType};

/// How fast a stale story recovers its novelty when it stops being shown.
pub const NOVELTY_RECOVERY_DAYS: f64 = 14.0;

/// Prior exposure of one story, loaded from `insight_history`.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub shown_count: u32,
    /// Unix epoch seconds of the last time the story was shown.
    pub last_shown_epoch: i64,
    /// Direction + magnitude decile at last showing; a change resets novelty.
    pub last_value_sig: String,
}

/// Novelty in [0, 1]: 1 = never seen; repeated showings push it toward 0;
/// time since the last showing lets it recover.
///
/// `novelty = 1 − (1 − 0.5^shown_count) · exp(−days_since_last / 14)`
pub fn novelty(shown_count: u32, days_since_last: f64) -> f64 {
    let staleness = 1.0 - 0.5_f64.powi(i32::try_from(shown_count).unwrap_or(i32::MAX));
    let recency = (-days_since_last.max(0.0) / NOVELTY_RECOVERY_DAYS).exp();
    (1.0 - staleness * recency).clamp(0.0, 1.0)
}

/// Compact signature of a finding's current value: direction + magnitude
/// decile. When this changes between runs, the story "developed" and gets its
/// novelty back even if the fingerprint matches.
pub fn value_signature(node: &AnalysisNode) -> String {
    let direction = match &node.analysis {
        AnalysisType::Anomaly { z_score, .. } => sign_char(*z_score),
        AnalysisType::Trend { slope, .. } => sign_char(*slope),
        AnalysisType::PeriodComparison { change_percent, .. }
        | AnalysisType::PeriodAnomaly { change_percent, .. }
        | AnalysisType::Segment { change_percent, .. } => sign_char(*change_percent),
        AnalysisType::ChangePoint {
            before_mean,
            after_mean,
            ..
        } => sign_char(after_mean - before_mean),
        AnalysisType::RankChange {
            previous_rank,
            new_rank,
            ..
        } => {
            if new_rank < previous_rank {
                '+'
            } else {
                '-'
            }
        },
        AnalysisType::ForecastDeviation {
            deviation_percent, ..
        } => sign_char(*deviation_percent),
        _ => '=',
    };
    let decile = (node.significance.clamp(0.0, 1.0) * 10.0).floor() as u8;
    format!("{direction}{decile}")
}

fn sign_char(v: f64) -> char {
    if v > 0.0 {
        '+'
    } else {
        '-'
    }
}

/// Apply novelty decay to every root with a fingerprint present in `history`,
/// recomputing its composite score. Call after dedup, before selection.
#[allow(clippy::implicit_hasher)]
pub fn apply_novelty(
    tree: &mut AnalysisTree,
    history: &HashMap<String, HistoryEntry>,
    now_epoch: i64,
) {
    if history.is_empty() {
        return;
    }
    let root_ids: Vec<usize> = tree.roots.iter().map(|r| r.0).collect();
    for idx in root_ids {
        let Some(node) = tree.nodes.get(idx) else {
            continue;
        };
        if node.fingerprint.is_empty() {
            continue;
        }
        let Some(entry) = history.get(&node.fingerprint) else {
            continue;
        };
        // A changed value signature means the story developed → stays novel.
        if value_signature(node) != entry.last_value_sig {
            continue;
        }
        let days = (now_epoch - entry.last_shown_epoch) as f64 / 86_400.0;
        let n = novelty(entry.shown_count, days);
        if let Some(target) = tree.nodes.get_mut(idx) {
            target.score_breakdown.novelty = n;
            target.significance = scoring::total(&target.score_breakdown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_shown_is_fully_novel() {
        assert!((novelty(0, 0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn just_shown_once_is_half_novel() {
        assert!((novelty(1, 0.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn repeated_showings_decay_further() {
        assert!(novelty(3, 0.0) < novelty(1, 0.0));
        // But never fully zero
        assert!(novelty(10, 0.0) > 0.0);
    }

    #[test]
    fn novelty_recovers_over_time() {
        let fresh = novelty(3, 0.0);
        let after_two_weeks = novelty(3, 14.0);
        let after_two_months = novelty(3, 60.0);
        assert!(after_two_weeks > fresh);
        assert!(after_two_months > after_two_weeks);
        assert!(after_two_months > 0.95);
    }
}

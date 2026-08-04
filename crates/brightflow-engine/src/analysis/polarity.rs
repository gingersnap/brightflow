//! Measure polarity → per-node sentiment.
//!
//! Rising revenue is good; rising churn is not. `apply_sentiment` walks a
//! finished tree and tags every directional finding whose measure has a
//! non-neutral polarity. Display-only in v1: this module writes sentiment onto
//! the tree for renderers and feeds nothing back into scoring.

use std::collections::HashMap;

use crate::analysis::select::measure_of;
use crate::analysis::tree::{AnalysisNode, AnalysisTree, AnalysisType, Sentiment, TrendDirection};
use crate::data::config::Polarity;

/// Which way a finding points, if it points anywhere.
///
/// Contract: this table mirrors the frontend's `directionOf` (`nodeMeta.ts`)
/// and the two must stay in lockstep — a unit test here pins the table so a
/// drift shows up as a failing test, not a silent disagreement.
pub fn direction_of(node: &AnalysisNode) -> Option<bool> {
    // true = up, false = down
    match &node.analysis {
        AnalysisType::Anomaly { z_score, .. } => Some(*z_score > 0.0),
        AnalysisType::PeriodComparison { change_percent, .. }
        | AnalysisType::PeriodAnomaly { change_percent, .. }
        | AnalysisType::Segment { change_percent, .. } => Some(*change_percent > 0.0),
        AnalysisType::Trend { direction, .. } => {
            Some(matches!(direction, TrendDirection::Increasing))
        },
        AnalysisType::ForecastDeviation {
            deviation_percent, ..
        } => Some(*deviation_percent > 0.0),
        AnalysisType::OutlierCluster { direction, .. } => Some(direction == "spike"),
        AnalysisType::ChangePoint {
            before_mean,
            after_mean,
            ..
        } => Some(after_mean > before_mean),
        AnalysisType::RankChange {
            previous_rank,
            new_rank,
            ..
        } => Some(new_rank < previous_rank),
        AnalysisType::TopDominance { .. }
        | AnalysisType::Concentration { .. }
        | AnalysisType::Correlation { .. }
        | AnalysisType::DistributionShift { .. }
        | AnalysisType::MembershipChange { .. }
        | AnalysisType::Seasonality { .. } => None,
    }
}

/// Tag every directional node whose measure carries a non-neutral polarity.
#[allow(clippy::implicit_hasher)] // internal map, never built with a custom hasher
pub fn apply_sentiment(tree: &mut AnalysisTree, polarity: &HashMap<String, Polarity>) {
    if polarity.is_empty() {
        return;
    }
    for node in &mut tree.nodes {
        let Some(p) = polarity.get(&measure_of(node)) else {
            continue;
        };
        let Some(up) = direction_of(node) else {
            continue;
        };
        node.sentiment = match (p, up) {
            (Polarity::HigherIsBetter, true) | (Polarity::LowerIsBetter, false) => {
                Some(Sentiment::Good)
            },
            (Polarity::HigherIsBetter, false) | (Polarity::LowerIsBetter, true) => {
                Some(Sentiment::Bad)
            },
            (Polarity::Neutral, _) => None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tree::AnalysisTree;

    fn trend(col: &str, up: bool) -> AnalysisType {
        AnalysisType::Trend {
            column: col.to_string(),
            direction: if up {
                TrendDirection::Increasing
            } else {
                TrendDirection::Decreasing
            },
            slope: 1.0,
            r_squared: 0.9,
            p_value: 0.001,
        }
    }

    /// Direction table parity with the frontend `directionOf` (nodeMeta.ts).
    #[test]
    fn direction_table_matches_frontend_port() {
        let mut tree = AnalysisTree::new();
        tree.add_root(trend("m", true), 0.9, String::new());
        tree.add_root(
            AnalysisType::Anomaly {
                column: "m".to_string(),
                value: 1.0,
                mean: 5.0,
                std_dev: 1.0,
                z_score: -4.0,
            },
            0.9,
            String::new(),
        );
        tree.add_root(
            AnalysisType::Seasonality {
                column: "m".to_string(),
                period_name: "weekly".to_string(),
                autocorrelation: 0.8,
                p_value: 0.01,
            },
            0.9,
            String::new(),
        );
        assert_eq!(direction_of(&tree.nodes[0]), Some(true));
        assert_eq!(direction_of(&tree.nodes[1]), Some(false));
        assert_eq!(direction_of(&tree.nodes[2]), None);
    }

    #[test]
    fn lower_is_better_flips_the_framing() {
        let mut tree = AnalysisTree::new();
        tree.add_root(trend("churn", true), 0.9, String::new());
        tree.add_root(trend("churn", false), 0.9, String::new());
        tree.add_root(trend("revenue", true), 0.9, String::new());
        tree.add_root(trend("untagged", true), 0.9, String::new());
        let mut polarity = HashMap::new();
        polarity.insert("churn".to_string(), Polarity::LowerIsBetter);
        polarity.insert("revenue".to_string(), Polarity::HigherIsBetter);
        apply_sentiment(&mut tree, &polarity);
        assert_eq!(tree.nodes[0].sentiment, Some(Sentiment::Bad));
        assert_eq!(tree.nodes[1].sentiment, Some(Sentiment::Good));
        assert_eq!(tree.nodes[2].sentiment, Some(Sentiment::Good));
        assert_eq!(tree.nodes[3].sentiment, None);
    }
}

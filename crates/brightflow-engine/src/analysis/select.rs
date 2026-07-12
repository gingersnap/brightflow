//! Final selection: diversity-aware top-N over the scored roots.
//!
//! Greedy MMR: repeatedly pick the root with the highest score after
//! penalizing similarity to already-selected roots. Without this, one loud
//! measure floods the whole top-N with restatements of the same story.

use crate::analysis::tree::{AnalysisNode, AnalysisTree, AnalysisType};

/// Multiplicative penalty per already-selected root sharing the measure.
const SAME_MEASURE_PENALTY: f64 = 0.6;
/// … sharing the detector type.
const SAME_TYPE_PENALTY: f64 = 0.8;
/// … sharing the dimension.
const SAME_DIMENSION_PENALTY: f64 = 0.8;

/// Rank the roots with diversity, write `rank` onto the selected nodes, and
/// truncate `tree.roots` to `max_results`.
struct Traits {
    measure: String,
    type_name: &'static str,
    dimension: Option<String>,
    base_score: f64,
}

pub fn select_top(tree: &mut AnalysisTree, max_results: usize) {
    let candidates: Vec<usize> = tree.roots.iter().map(|r| r.0).collect();
    if candidates.is_empty() {
        return;
    }

    let traits: Vec<(usize, Traits)> = candidates
        .iter()
        .filter_map(|&idx| {
            let node = tree.nodes.get(idx)?;
            Some((
                idx,
                Traits {
                    measure: measure_of(node),
                    type_name: node.analysis.kind_name(),
                    dimension: dimension_of(node),
                    base_score: node.significance,
                },
            ))
        })
        .collect();

    let mut selected: Vec<usize> = Vec::new(); // indices into `traits`
    let mut remaining: Vec<usize> = (0..traits.len()).collect();

    while selected.len() < max_results && !remaining.is_empty() {
        let mut best_pos = 0;
        let mut best_score = f64::NEG_INFINITY;
        for (pos, &ti) in remaining.iter().enumerate() {
            let t = &traits[ti].1;
            let mut score = t.base_score;
            for &si in &selected {
                let s = &traits[si].1;
                if s.measure == t.measure {
                    score *= SAME_MEASURE_PENALTY;
                }
                if s.type_name == t.type_name {
                    score *= SAME_TYPE_PENALTY;
                }
                if let (Some(a), Some(b)) = (&s.dimension, &t.dimension) {
                    if a == b {
                        score *= SAME_DIMENSION_PENALTY;
                    }
                }
            }
            if score > best_score {
                best_score = score;
                best_pos = pos;
            }
        }
        selected.push(remaining.remove(best_pos));
    }

    // Write ranks and rebuild the roots list in selection order.
    let mut new_roots = Vec::with_capacity(selected.len());
    for (rank0, &si) in selected.iter().enumerate() {
        let node_idx = traits[si].0;
        if let Some(node) = tree.nodes.get_mut(node_idx) {
            node.rank = Some(u32::try_from(rank0 + 1).unwrap_or(u32::MAX));
        }
        new_roots.push(crate::analysis::tree::NodeId(node_idx));
    }
    tree.roots = new_roots;
}

/// The measure a finding is about (for diversity grouping).
pub fn measure_of(node: &AnalysisNode) -> String {
    match &node.analysis {
        AnalysisType::Anomaly { column, .. }
        | AnalysisType::Trend { column, .. }
        | AnalysisType::PeriodComparison { column, .. }
        | AnalysisType::PeriodAnomaly { column, .. }
        | AnalysisType::Seasonality { column, .. }
        | AnalysisType::ForecastDeviation { column, .. }
        | AnalysisType::Concentration { column, .. }
        | AnalysisType::DistributionShift { column, .. }
        | AnalysisType::ChangePoint { column, .. } => column.clone(),
        AnalysisType::Segment { target_column, .. } => target_column.clone(),
        AnalysisType::Correlation { column_a, .. } => column_a.clone(),
        AnalysisType::RankChange { measure, .. } | AnalysisType::TopDominance { measure, .. } => {
            measure.clone()
        },
        AnalysisType::OutlierCluster { columns, .. } => columns.join(","),
        AnalysisType::MembershipChange { segment_column, .. } => segment_column.clone(),
    }
}

/// The dimension a finding slices on, if any.
pub fn dimension_of(node: &AnalysisNode) -> Option<String> {
    match &node.analysis {
        AnalysisType::Segment { segment_column, .. }
        | AnalysisType::Concentration { segment_column, .. }
        | AnalysisType::MembershipChange { segment_column, .. } => Some(segment_column.clone()),
        AnalysisType::RankChange { dimension, .. }
        | AnalysisType::TopDominance { dimension, .. } => Some(dimension.clone()),
        _ => node.filter_chain.first().map(|f| f.column.clone()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::suboptimal_flops)]
mod tests {
    use super::*;
    use crate::analysis::tree::{AnalysisTree, TrendDirection};

    fn trend(col: &str) -> AnalysisType {
        AnalysisType::Trend {
            column: col.to_string(),
            direction: TrendDirection::Increasing,
            slope: 1.0,
            r_squared: 0.9,
            p_value: 0.001,
        }
    }

    fn anomaly(col: &str) -> AnalysisType {
        AnalysisType::Anomaly {
            column: col.to_string(),
            value: 10.0,
            mean: 1.0,
            std_dev: 1.0,
            z_score: 9.0,
        }
    }

    #[test]
    fn diversity_prefers_second_measure_over_restatement() {
        let mut tree = AnalysisTree::new();
        // Two findings about "revenue" (0.9, 0.85) and one about "users" (0.8).
        tree.add_root(trend("revenue"), 0.9, "a".into());
        tree.add_root(anomaly("revenue"), 0.85, "b".into());
        tree.add_root(trend("users"), 0.8, "c".into());
        select_top(&mut tree, 2);
        assert_eq!(tree.roots.len(), 2);
        let picked: Vec<String> = tree
            .roots
            .iter()
            .map(|r| measure_of(&tree.nodes[r.0]))
            .collect();
        assert_eq!(
            picked,
            vec!["revenue", "users"],
            "0.85·0.6 < 0.8 → users wins slot 2"
        );
        assert_eq!(tree.nodes[tree.roots[0].0].rank, Some(1));
        assert_eq!(tree.nodes[tree.roots[1].0].rank, Some(2));
    }

    #[test]
    fn truncates_to_max_results() {
        let mut tree = AnalysisTree::new();
        for i in 0..10 {
            tree.add_root(
                trend(&format!("m{i}")),
                1.0 - f64::from(i) * 0.05,
                "x".into(),
            );
        }
        select_top(&mut tree, 3);
        assert_eq!(tree.roots.len(), 3);
    }
}

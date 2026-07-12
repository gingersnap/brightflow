//! Dedup / clustering of analysis findings.
//!
//! Runs after scoring, before final ranking.
//!
//! Two passes:
//! 1. **Lattice collapse**: a child finding whose score is not materially
//!    larger than its parent is suppressed (parent's drill-down already covers it).
//! 2. **Story grouping**: roots that share a target column AND overlap in
//!    period or segment are merged — the dominant root absorbs the others
//!    as children.

use std::collections::HashMap;

use crate::analysis::tree::{AnalysisNode, AnalysisTree, AnalysisType, NodeId};

/// Multiplier above which a child must score to survive against its parent.
/// 1.2 means: a child must score at least 1.2× its parent to be kept.
const CHILD_SUPERIORITY_MULTIPLIER: f64 = 1.2;

/// Apply both lattice collapse and story grouping. Mutates the tree in place
/// — does not delete nodes (that would invalidate NodeIds), just trims the
/// `roots` list and the `children` lists.
pub fn dedup(tree: &mut AnalysisTree) {
    lattice_collapse(tree);
    story_group_roots(tree);
}

/// Suppress children whose score is not materially larger than their parent.
fn lattice_collapse(tree: &mut AnalysisTree) {
    let n = tree.nodes.len();
    let mut new_children: Vec<Vec<NodeId>> = vec![Vec::new(); n];

    for (idx, node) in tree.nodes.iter().enumerate() {
        let parent_score = node.significance;
        for child_id in &node.children {
            let Some(child) = tree.nodes.get(child_id.0) else {
                continue;
            };
            if parent_score <= 0.0
                || child.significance >= parent_score * CHILD_SUPERIORITY_MULTIPLIER
            {
                new_children[idx].push(*child_id);
            }
        }
    }

    for (idx, children) in new_children.into_iter().enumerate() {
        if let Some(node) = tree.nodes.get_mut(idx) {
            node.children = children;
        }
    }
}

/// Cluster roots by target column. Within each cluster, the highest-scoring
/// root absorbs the others as children. The resulting `roots` list contains
/// only the dominant root from each cluster.
fn story_group_roots(tree: &mut AnalysisTree) {
    if tree.roots.is_empty() {
        return;
    }

    let mut groups: HashMap<String, Vec<NodeId>> = HashMap::new();
    let mut keyless: Vec<NodeId> = Vec::new();

    for root_id in &tree.roots {
        let Some(node) = tree.nodes.get(root_id.0) else {
            continue;
        };
        match story_key(node) {
            Some(key) => groups.entry(key).or_default().push(*root_id),
            None => keyless.push(*root_id),
        }
    }

    let mut new_roots: Vec<NodeId> = keyless;

    for (_key, mut ids) in groups {
        if ids.len() == 1 {
            new_roots.push(ids[0]);
            continue;
        }
        // Pick the highest-scoring root as the dominant
        ids.sort_by(|a, b| {
            let sa = tree.nodes.get(a.0).map_or(0.0, |n| n.significance);
            let sb = tree.nodes.get(b.0).map_or(0.0, |n| n.significance);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        let dominant = ids[0];
        for sub in ids.iter().skip(1) {
            // Move sub under dominant
            if let Some(n) = tree.nodes.get_mut(sub.0) {
                n.parent_id = Some(dominant);
            }
            if let Some(d) = tree.nodes.get_mut(dominant.0) {
                d.children.push(*sub);
            }
        }
        new_roots.push(dominant);
    }

    // Preserve original ordering: sort by significance descending
    new_roots.sort_by(|a, b| {
        let sa = tree.nodes.get(a.0).map_or(0.0, |n| n.significance);
        let sb = tree.nodes.get(b.0).map_or(0.0, |n| n.significance);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    tree.roots = new_roots;
}

/// A "story key" groups findings about the same metric together. Findings
/// without a clear single column (correlation, outlier cluster) are not grouped.
fn story_key(node: &AnalysisNode) -> Option<String> {
    match &node.analysis {
        AnalysisType::Anomaly { column, .. }
        | AnalysisType::Trend { column, .. }
        | AnalysisType::PeriodComparison { column, .. }
        | AnalysisType::PeriodAnomaly { column, .. }
        | AnalysisType::Seasonality { column, .. }
        | AnalysisType::ForecastDeviation { column, .. }
        | AnalysisType::Concentration { column, .. }
        | AnalysisType::DistributionShift { column, .. }
        | AnalysisType::ChangePoint { column, .. } => Some(column.clone()),
        AnalysisType::Segment { target_column, .. } => Some(target_column.clone()),
        // Derived-series stories key on their full recipe: same dimension
        // value can legitimately carry a rank story AND a dominance story.
        AnalysisType::RankChange {
            dimension, value, ..
        } => Some(format!("rank:{dimension}={value}")),
        AnalysisType::TopDominance {
            dimension, value, ..
        } => Some(format!("dominance:{dimension}={value}")),
        AnalysisType::Correlation { .. }
        | AnalysisType::OutlierCluster { .. }
        | AnalysisType::MembershipChange { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tree::TrendDirection;

    fn make_anomaly(col: &str, z: f64) -> AnalysisType {
        AnalysisType::Anomaly {
            column: col.to_string(),
            value: z,
            mean: 0.0,
            std_dev: 1.0,
            z_score: z,
        }
    }

    #[test]
    fn story_grouping_merges_same_column_roots() {
        let mut tree = AnalysisTree::new();
        let a = tree.add_root(make_anomaly("revenue", 3.0), 3.0, "a".into());
        let b = tree.add_root(
            AnalysisType::Trend {
                column: "revenue".into(),
                direction: TrendDirection::Increasing,
                slope: 0.1,
                r_squared: 0.9,
                p_value: 0.001,
            },
            5.0,
            "b".into(),
        );
        let c = tree.add_root(make_anomaly("cost", 2.0), 2.0, "c".into());
        dedup(&mut tree);
        // revenue cluster collapses to 1 root, cost stays 1 root → 2 roots total
        assert_eq!(tree.roots.len(), 2);
        // The trend (higher score) should be the dominant root for revenue
        let revenue_root = tree
            .roots
            .iter()
            .find(|r| matches!(&tree.nodes[r.0].analysis, AnalysisType::Trend { .. }));
        assert!(revenue_root.is_some());
        // Suppress unused
        let _ = (a, b, c);
    }

    #[test]
    fn lattice_collapse_drops_weak_children() {
        let mut tree = AnalysisTree::new();
        let parent = tree.add_root(make_anomaly("x", 5.0), 10.0, "p".into());
        // Weak child (same score as parent)
        let _ = tree.add_child(parent, make_anomaly("x", 5.0), 10.0, "c1".into());
        // Strong child (1.5× parent)
        let _ = tree.add_child(parent, make_anomaly("x", 5.0), 15.0, "c2".into());
        dedup(&mut tree);
        assert_eq!(tree.nodes[parent.0].children.len(), 1);
    }
}

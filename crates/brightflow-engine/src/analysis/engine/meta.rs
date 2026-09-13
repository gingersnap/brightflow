//! Insight metadata for the legacy (non-derived-series) detector sites.
//!
//! The derived-series pass attaches why/provenance/fingerprint through its
//! `SeriesFrame` recipe; the ten older sites (period comparison, seasonality,
//! concentration, …) built bare nodes with empty fingerprints — which kept
//! them out of novelty decay, curation, and insight history entirely. This
//! helper gives them the same treatment using the same 6-slot fingerprint
//! scheme (`Provenance::fingerprint`), so recurring stories match across runs.
//!
//! Dimension-scoped detectors use the wildcard filter value `"*"` — the story
//! is "concentration in revenue by region", not one specific region value.
//! Detector strings must stay distinct from the derived-pass detectors
//! (`trend`, `change_point`, `last_point`, `rank_change`, `top_dominance`) so
//! the two passes can never fingerprint-collide (an integration test pins
//! this).

use crate::analysis::candidates::{Aggregation, FilterSpec, MeasureRef, Provenance};
use crate::analysis::scoring::{self, ScoringContext};
use crate::analysis::tree::{AnalysisTree, AnalysisType, NodeData, NodeId};

/// Metadata for one legacy detector root (the `set_legacy_meta` slots).
pub(super) struct RootMeta<'a> {
    pub detector: &'a str,
    pub measure: MeasureRef,
    pub agg: Aggregation,
    pub dimension: Option<&'a str>,
    pub granularity: &'a str,
    pub why: String,
}

/// Score → floor-check → add root → attach legacy meta: the tail every
/// legacy detector arm shares. `None` = the finding scored below the floor
/// and no node was added (callers use this to skip follow-up drill tasks).
pub(super) fn add_scored_root(
    tree: &mut AnalysisTree,
    ctx: &ScoringContext,
    analysis: AnalysisType,
    description: String,
    data: Option<NodeData>,
    meta: RootMeta<'_>,
) -> Option<NodeId> {
    let breakdown = scoring::score(&analysis, ctx);
    if !scoring::passes_floor(&breakdown, ctx) {
        return None;
    }
    let score = scoring::total(&breakdown);
    let node_id = tree.add_root_full(analysis, score, breakdown, description, data);
    set_legacy_meta(
        tree,
        node_id,
        meta.detector,
        meta.measure,
        meta.agg,
        meta.dimension,
        meta.granularity,
        meta.why,
    );
    Some(node_id)
}

/// Attach why/provenance/depth/fingerprint to a legacy detector's root.
pub(super) fn set_legacy_meta(
    tree: &mut AnalysisTree,
    id: NodeId,
    detector: &str,
    measure: MeasureRef,
    agg: Aggregation,
    dimension: Option<&str>,
    granularity: &str,
    why: String,
) {
    let filters = dimension
        .map(|d| {
            vec![FilterSpec {
                column: d.to_string(),
                value: "*".to_string(),
            }]
        })
        .unwrap_or_default();
    let provenance = Provenance {
        measure,
        aggregation: agg,
        filters,
        derivations: Vec::new(),
        granularity: granularity.to_string(),
    };
    tree.set_insight_meta(
        id,
        why,
        provenance.steps(),
        provenance.depth(),
        provenance.fingerprint(detector),
    );
}

/// Measure ref for a column name, treating the synthetic "rows" measure as
/// row count.
pub(super) fn measure_ref(column: &str) -> MeasureRef {
    if column == "rows" {
        MeasureRef::RowCount
    } else {
        MeasureRef::Column(column.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tree::TrendDirection;

    fn trend(p_value: f64, r_squared: f64) -> AnalysisType {
        AnalysisType::Trend {
            column: "revenue".to_string(),
            direction: TrendDirection::Increasing,
            slope: 1.0,
            r_squared,
            p_value,
        }
    }

    #[test]
    fn add_scored_root_adds_passing_and_floors_weak_findings() {
        let ctx = ScoringContext::new();
        let mut tree = AnalysisTree::new();

        let meta = |why: &str| RootMeta {
            detector: "trend_legacy",
            measure: MeasureRef::Column("revenue".to_string()),
            agg: Aggregation::Mean,
            dimension: None,
            granularity: "day",
            why: why.to_string(),
        };

        let strong = add_scored_root(
            &mut tree,
            &ctx,
            trend(0.001, 0.9),
            "strong trend".to_string(),
            None,
            meta("clean line"),
        );
        let id = strong.expect("a clean, significant trend must pass the floor");
        let node = &tree.nodes[id.0];
        assert_eq!(node.why, "clean line", "legacy meta must be attached");
        assert!(!node.fingerprint.is_empty(), "fingerprint must be attached");

        let weak = add_scored_root(
            &mut tree,
            &ctx,
            trend(0.95, 0.001),
            "noise".to_string(),
            None,
            meta("noise"),
        );
        assert!(weak.is_none(), "a floored finding must not add a node");
        assert_eq!(tree.nodes.len(), 1, "tree unchanged by the floored finding");
    }
}

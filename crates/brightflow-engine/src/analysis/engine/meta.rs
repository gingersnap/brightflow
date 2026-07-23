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
//! the two passes can never fingerprint-collide (see the collision test in
//! `tests/drivers_report.rs`).

use crate::analysis::candidates::{Aggregation, FilterSpec, MeasureRef, Provenance};
use crate::analysis::tree::{AnalysisTree, NodeId};

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

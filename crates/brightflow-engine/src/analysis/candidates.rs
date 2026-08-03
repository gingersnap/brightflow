//! Candidate enumeration: subspace × measure × transform → derived series.
//!
//! The old engine only tested pre-existing numeric columns at raw row level,
//! so the series that actually carry stories — counts/day, engagement/day,
//! per-segment shares, ranks — never existed. This module materializes them:
//!
//!   (filters up to depth 2) × (row count | column sums/means) × (identity |
//!   Δ | %Δ | share-of-total | rank-among-siblings)
//!
//! Enumeration is bounded by an [`EnumerationBudget`] and pruned by *impact*
//! (the share of total row volume a slice covers). Impact is anti-monotonic
//! under filtering — a child slice can never cover more rows than its parent
//! — which is what makes the pruning sound (QuickInsights).

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::analysis::engine::ColumnCache;
use crate::analysis::tree::ProvenanceStep;
use crate::nlp::fingerprint::fingerprint;

// ─── Provenance ───────────────────────────────────────────────────────────────

/// What is being measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeasureRef {
    /// Number of rows — the measure the old engine never had.
    RowCount,
    /// A numeric column.
    Column(String),
}

impl MeasureRef {
    pub fn name(&self) -> &str {
        match self {
            Self::RowCount => "rows",
            Self::Column(c) => c,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregation {
    Count,
    Sum,
    Mean,
}

impl Aggregation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Mean => "mean",
        }
    }
}

/// Series-to-series transforms (the "derive" step of composition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derivation {
    /// Point-to-point difference.
    Delta,
    /// Point-to-point percent change.
    PctChange,
    /// Slice value divided by the whole-table value, per period.
    ShareOfTotal,
    /// Rank of this slice among its dimension's siblings, per period (1 = top).
    RankAmongSiblings,
}

impl Derivation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Delta => "delta",
            Self::PctChange => "pct_change",
            Self::ShareOfTotal => "share_of_total",
            Self::RankAmongSiblings => "rank",
        }
    }

    pub fn human(self) -> &'static str {
        match self {
            Self::Delta => "period-over-period change",
            Self::PctChange => "period-over-period % change",
            Self::ShareOfTotal => "share of total",
            Self::RankAmongSiblings => "rank among peers",
        }
    }
}

/// One `column = value` filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterSpec {
    pub column: String,
    pub value: String,
}

/// Full recipe for a derived series — enough to recompute it, label it, and
/// fingerprint it.
#[derive(Debug, Clone)]
pub struct Provenance {
    pub measure: MeasureRef,
    pub aggregation: Aggregation,
    pub filters: Vec<FilterSpec>,
    pub derivations: Vec<Derivation>,
    /// Period granularity name, e.g. "day", "week".
    pub granularity: String,
}

impl Provenance {
    /// Human-readable label, e.g.
    /// `count of rows per day where region=EU (share of total)`.
    pub fn label(&self) -> String {
        let mut s = format!(
            "{} of {} per {}",
            self.aggregation.name(),
            self.measure.name(),
            self.granularity
        );
        if !self.filters.is_empty() {
            let filters = self
                .filters
                .iter()
                .map(|f| format!("{}={}", f.column, f.value))
                .collect::<Vec<_>>()
                .join(" and ");
            let _ = write!(s, " where {filters}");
        }
        for d in &self.derivations {
            let _ = write!(s, ", {}", d.human());
        }
        s
    }

    /// UI chips: one step per pipeline stage.
    pub fn steps(&self) -> Vec<ProvenanceStep> {
        let mut steps = vec![ProvenanceStep {
            kind: "measure".to_string(),
            label: format!(
                "{}({}) per {}",
                self.aggregation.name(),
                self.measure.name(),
                self.granularity
            ),
        }];
        for f in &self.filters {
            steps.push(ProvenanceStep {
                kind: "filter".to_string(),
                label: format!("{}={}", f.column, f.value),
            });
        }
        for d in &self.derivations {
            steps.push(ProvenanceStep {
                kind: "derive".to_string(),
                label: d.human().to_string(),
            });
        }
        steps
    }

    /// Composition depth: 1 for a bare aggregate, +1 per filter/derivation.
    /// Depth-1 findings ("total revenue has a mean") are inherently trivial
    /// and get a stricter significance floor downstream.
    pub fn depth(&self) -> u8 {
        u8::try_from(1 + self.filters.len() + self.derivations.len()).unwrap_or(u8::MAX)
    }

    /// Stable story fingerprint (see `nlp::fingerprint`): excludes the
    /// observed values/period so recurring stories match across runs.
    pub fn fingerprint(&self, detector: &str) -> String {
        let filters = self
            .filters
            .iter()
            .map(|f| format!("{}={}", f.column, f.value))
            .collect::<Vec<_>>()
            .join(",");
        let derivations = self
            .derivations
            .iter()
            .map(|d| d.name())
            .collect::<Vec<_>>()
            .join(",");
        fingerprint(&[
            detector,
            self.measure.name(),
            self.aggregation.name(),
            &derivations,
            &filters,
            &self.granularity,
        ])
    }
}

/// A materialized derived series, one point per period.
#[derive(Debug, Clone)]
pub struct SeriesFrame {
    /// Sorted period labels.
    pub labels: Vec<String>,
    pub values: Vec<f64>,
    /// Rows behind each point (for confidence downstream).
    pub counts: Vec<usize>,
    pub provenance: Provenance,
    /// Share of total row volume covered by this slice, in [0, 1].
    /// Whole-table series have impact 1.0.
    pub impact: f64,
}

// ─── Budget ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnumerationBudget {
    /// Hard cap on materialized series.
    pub max_series: usize,
    /// Slices covering less than this share of rows are pruned (and all their
    /// children with them — impact is anti-monotonic).
    pub min_impact: f64,
    /// Maximum number of stacked filters (subspace depth).
    pub max_subspace_depth: usize,
    /// Per dimension, only the most frequent N values become slices.
    pub top_values_per_dim: usize,
}

impl Default for EnumerationBudget {
    fn default() -> Self {
        Self {
            max_series: 20_000,
            min_impact: 0.02,
            max_subspace_depth: 2,
            top_values_per_dim: 12,
        }
    }
}

/// What the enumeration actually did — logged so silent truncation can't
/// masquerade as full coverage.
#[derive(Debug, Default, Clone)]
pub struct EnumerationStats {
    pub series_emitted: usize,
    pub slices_pruned_by_impact: usize,
    pub series_dropped_by_budget: usize,
}

// ─── Dimension index ──────────────────────────────────────────────────────────

/// Per-slice aggregates: one entry per period.
#[derive(Debug, Clone)]
pub struct SliceAgg {
    pub row_counts: Vec<usize>,
    /// measure column → per-period sums
    pub sums: HashMap<String, Vec<f64>>,
    /// measure column → per-period sums of squares — with `sums` and
    /// `row_counts` this yields mean and variance per period, which is what
    /// Welch tests (Drivers) need without a second row pass.
    pub sum_squares: HashMap<String, Vec<f64>>,
}

impl SliceAgg {
    fn new(n_periods: usize, measures: &[String]) -> Self {
        Self {
            row_counts: vec![0; n_periods],
            sums: measures
                .iter()
                .map(|m| (m.clone(), vec![0.0; n_periods]))
                .collect(),
            sum_squares: measures
                .iter()
                .map(|m| (m.clone(), vec![0.0; n_periods]))
                .collect(),
        }
    }

    pub fn total_rows(&self) -> usize {
        self.row_counts.iter().sum()
    }
}

/// One dimension value's slice.
#[derive(Debug, Clone)]
pub struct SliceEntry {
    pub dimension: String,
    pub value: String,
    pub agg: SliceAgg,
}

/// One pass over the rows → per-period aggregates for the whole table and for
/// every (dimension, top value) slice. Makes depth-1 slice series O(1).
///
/// **Limitation — null segment values are excluded from slices.** Empty-string
/// dimension values are dropped when picking top values, so rows with a null
/// segment never form a slice of their own in the derived-series pass or in
/// Drivers. They still count toward whole-table totals, which produces a visible
/// asymmetry: a delta driven mostly by null-segment rows shows a total change
/// that no listed driver explains. They are dropped because "" is not a value a
/// reader recognizes as a segment. (Segment attribution in Review does label them
/// "(blank)" — doing the same here, as an explicit bucket rather than a silent
/// omission, would lift it.)
pub struct DimensionIndex {
    pub periods: Vec<String>,
    pub total: SliceAgg,
    pub slices: Vec<SliceEntry>,
    pub total_rows: usize,
    /// Per dimension: row → index into that dimension's top-value list
    /// (None = not a top value). Enables cheap depth-2 passes.
    row_values: HashMap<String, Vec<Option<u16>>>,
    /// Per dimension: the ordered top values.
    top_values: HashMap<String, Vec<String>>,
    measures: Vec<String>,
    period_of_row: Vec<Option<u32>>,
}

impl DimensionIndex {
    pub fn build(
        cache: &ColumnCache,
        period_labels: &[Option<String>],
        budget: &EnumerationBudget,
    ) -> Self {
        // Sorted unique periods
        let mut periods: Vec<String> = period_labels
            .iter()
            .flatten()
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        periods.sort();
        let period_idx: HashMap<&str, u32> = periods
            .iter()
            .enumerate()
            .map(|(i, p)| (p.as_str(), u32::try_from(i).unwrap_or(u32::MAX)))
            .collect();
        let period_of_row: Vec<Option<u32>> = period_labels
            .iter()
            .map(|p| p.as_ref().and_then(|p| period_idx.get(p.as_str()).copied()))
            .collect();

        let measures: Vec<String> = cache.numeric.keys().cloned().collect();
        let n_periods = periods.len();
        let n_rows = period_labels.len();

        // Whole-table aggregates
        let mut total = SliceAgg::new(n_periods, &measures);
        for (row, p) in period_of_row.iter().enumerate() {
            let Some(p) = p else { continue };
            let p = *p as usize;
            total.row_counts[p] += 1;
            for m in &measures {
                if let Some(vals) = cache.numeric.get(m) {
                    if let Some(v) = vals.get(row) {
                        if let Some(sums) = total.sums.get_mut(m) {
                            sums[p] += v;
                        }
                        if let Some(sq) = total.sum_squares.get_mut(m) {
                            sq[p] += v * v;
                        }
                    }
                }
            }
        }

        // Top values per dimension
        let mut top_values: HashMap<String, Vec<String>> = HashMap::new();
        let mut row_values: HashMap<String, Vec<Option<u16>>> = HashMap::new();
        let mut slices: Vec<SliceEntry> = Vec::new();

        for (dim, values) in &cache.dimension {
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for v in values {
                if !v.is_empty() {
                    *counts.entry(v.as_str()).or_insert(0) += 1;
                }
            }
            let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            let top: Vec<String> = ranked
                .iter()
                .take(budget.top_values_per_dim)
                .map(|(v, _)| (*v).to_string())
                .collect();
            let value_idx: HashMap<&str, u16> = top
                .iter()
                .enumerate()
                .map(|(i, v)| (v.as_str(), u16::try_from(i).unwrap_or(u16::MAX)))
                .collect();

            // Per-row membership + per-slice aggregates in one pass
            let mut membership: Vec<Option<u16>> = vec![None; n_rows];
            let mut aggs: Vec<SliceAgg> = top
                .iter()
                .map(|_| SliceAgg::new(n_periods, &measures))
                .collect();
            for (row, v) in values.iter().enumerate() {
                let Some(&vi) = value_idx.get(v.as_str()) else {
                    continue;
                };
                membership[row] = Some(vi);
                let Some(Some(p)) = period_of_row.get(row) else {
                    continue;
                };
                let p = *p as usize;
                let agg = &mut aggs[vi as usize];
                agg.row_counts[p] += 1;
                for m in &measures {
                    if let Some(vals) = cache.numeric.get(m) {
                        if let Some(val) = vals.get(row) {
                            if let Some(sums) = agg.sums.get_mut(m) {
                                sums[p] += val;
                            }
                            if let Some(sq) = agg.sum_squares.get_mut(m) {
                                sq[p] += val * val;
                            }
                        }
                    }
                }
            }

            for (vi, agg) in aggs.into_iter().enumerate() {
                slices.push(SliceEntry {
                    dimension: dim.clone(),
                    value: top[vi].clone(),
                    agg,
                });
            }
            top_values.insert(dim.clone(), top);
            row_values.insert(dim.clone(), membership);
        }

        let total_rows = total.total_rows();
        Self {
            periods,
            total,
            slices,
            total_rows,
            row_values,
            top_values,
            measures,
            period_of_row,
        }
    }

    /// Depth-2 aggregates for a dimension pair, restricted to top values.
    /// Returns (value1, value2, agg) triples. One row pass per pair.
    ///
    /// **Limitation — this is why Drivers depth is one dimension.** Only counts
    /// are accumulated here; measure sums (and sums of squares) are deliberately
    /// skipped at depth 2 to keep the row pass cheap. So the moments a Welch test
    /// needs don't exist for a pair, and the delta decomposition can only rank
    /// single-dimension slices — "region=EU AND channel=web drove it" is not
    /// expressible yet. Accumulating the same moments here as at depth 1 would
    /// lift it, at the cost of a wider pass over every top-value pair.
    fn pair_aggregates(&self, dim_a: &str, dim_b: &str) -> Vec<(String, String, SliceAgg)> {
        let (Some(rows_a), Some(rows_b)) = (self.row_values.get(dim_a), self.row_values.get(dim_b))
        else {
            return Vec::new();
        };
        let (Some(top_a), Some(top_b)) = (self.top_values.get(dim_a), self.top_values.get(dim_b))
        else {
            return Vec::new();
        };
        let n_periods = self.periods.len();
        let mut aggs: HashMap<(u16, u16), SliceAgg> = HashMap::new();
        for (row, p) in self.period_of_row.iter().enumerate() {
            let Some(p) = p else { continue };
            let (Some(Some(va)), Some(Some(vb))) = (rows_a.get(row), rows_b.get(row)) else {
                continue;
            };
            let agg = aggs
                .entry((*va, *vb))
                .or_insert_with(|| SliceAgg::new(n_periods, &self.measures));
            let p = *p as usize;
            agg.row_counts[p] += 1;
            // Measure sums intentionally skipped at depth 2 — row counts carry
            // the volume story and keep the pass cheap; deepen later if needed.
        }
        aggs.into_iter()
            .filter_map(|((va, vb), agg)| {
                Some((
                    top_a.get(va as usize)?.clone(),
                    top_b.get(vb as usize)?.clone(),
                    agg,
                ))
            })
            .collect()
    }
}

// ─── Enumeration ──────────────────────────────────────────────────────────────

/// Materialize the candidate series under the budget.
pub fn enumerate_series(
    index: &DimensionIndex,
    granularity: &str,
    budget: &EnumerationBudget,
) -> (Vec<SeriesFrame>, EnumerationStats) {
    let mut out: Vec<SeriesFrame> = Vec::new();
    let mut stats = EnumerationStats::default();
    if index.periods.len() < 3 || index.total_rows == 0 {
        return (out, stats);
    }

    let push = |frame: SeriesFrame, acc: &mut Vec<SeriesFrame>, tally: &mut EnumerationStats| {
        if acc.len() >= budget.max_series {
            tally.series_dropped_by_budget += 1;
        } else {
            tally.series_emitted += 1;
            acc.push(frame);
        }
    };

    // Whole-table base series + Δ/%Δ derivations
    for base in base_series_for(&index.total, index, granularity, &[], 1.0) {
        for derived in with_derivations(&base) {
            push(derived, &mut out, &mut stats);
        }
        push(base, &mut out, &mut stats);
    }

    // Depth-1 slices
    for slice in &index.slices {
        let impact = slice.agg.total_rows() as f64 / index.total_rows as f64;
        if impact < budget.min_impact {
            stats.slices_pruned_by_impact += 1;
            continue;
        }
        let filters = vec![FilterSpec {
            column: slice.dimension.clone(),
            value: slice.value.clone(),
        }];
        for base in base_series_for(&slice.agg, index, granularity, &filters, impact) {
            // Share-of-total against the matching whole-table series
            if let Some(share) = share_of_total(&base, index) {
                push(share, &mut out, &mut stats);
            }
            for derived in with_derivations(&base) {
                push(derived, &mut out, &mut stats);
            }
            push(base, &mut out, &mut stats);
        }
    }

    // Rank-among-siblings per dimension (row-count volume ranks)
    for frame in rank_series(index, granularity, budget) {
        push(frame, &mut out, &mut stats);
    }

    // Depth-2 subspaces (row counts only, impact-pruned via parents)
    if budget.max_subspace_depth >= 2 {
        let dims: Vec<&String> = index.top_values.keys().collect();
        for i in 0..dims.len() {
            for j in (i + 1)..dims.len() {
                if out.len() >= budget.max_series {
                    break;
                }
                for (va, vb, agg) in index.pair_aggregates(dims[i], dims[j]) {
                    let impact = agg.total_rows() as f64 / index.total_rows as f64;
                    if impact < budget.min_impact {
                        stats.slices_pruned_by_impact += 1;
                        continue;
                    }
                    let provenance = Provenance {
                        measure: MeasureRef::RowCount,
                        aggregation: Aggregation::Count,
                        filters: vec![
                            FilterSpec {
                                column: dims[i].clone(),
                                value: va,
                            },
                            FilterSpec {
                                column: dims[j].clone(),
                                value: vb,
                            },
                        ],
                        derivations: Vec::new(),
                        granularity: granularity.to_string(),
                    };
                    let base = SeriesFrame {
                        labels: index.periods.clone(),
                        values: agg.row_counts.iter().map(|c| *c as f64).collect(),
                        counts: agg.row_counts.clone(),
                        provenance,
                        impact,
                    };
                    for derived in with_derivations(&base) {
                        push(derived, &mut out, &mut stats);
                    }
                    push(base, &mut out, &mut stats);
                }
            }
        }
    }

    (out, stats)
}

/// Base (underived) series for one slice: row count + per-measure sum & mean.
fn base_series_for(
    agg: &SliceAgg,
    index: &DimensionIndex,
    granularity: &str,
    filters: &[FilterSpec],
    impact: f64,
) -> Vec<SeriesFrame> {
    let mut out = Vec::new();
    let labels = index.periods.clone();

    out.push(SeriesFrame {
        labels: labels.clone(),
        values: agg.row_counts.iter().map(|c| *c as f64).collect(),
        counts: agg.row_counts.clone(),
        provenance: Provenance {
            measure: MeasureRef::RowCount,
            aggregation: Aggregation::Count,
            filters: filters.to_vec(),
            derivations: Vec::new(),
            granularity: granularity.to_string(),
        },
        impact,
    });

    for (measure, sums) in &agg.sums {
        out.push(SeriesFrame {
            labels: labels.clone(),
            values: sums.clone(),
            counts: agg.row_counts.clone(),
            provenance: Provenance {
                measure: MeasureRef::Column(measure.clone()),
                aggregation: Aggregation::Sum,
                filters: filters.to_vec(),
                derivations: Vec::new(),
                granularity: granularity.to_string(),
            },
            impact,
        });
        let means: Vec<f64> = sums
            .iter()
            .zip(agg.row_counts.iter())
            .map(|(s, c)| if *c == 0 { f64::NAN } else { s / *c as f64 })
            .collect();
        out.push(SeriesFrame {
            labels: labels.clone(),
            values: means,
            counts: agg.row_counts.clone(),
            provenance: Provenance {
                measure: MeasureRef::Column(measure.clone()),
                aggregation: Aggregation::Mean,
                filters: filters.to_vec(),
                derivations: Vec::new(),
                granularity: granularity.to_string(),
            },
            impact,
        });
    }
    out
}

/// Δ and %Δ variants of a base series.
fn with_derivations(base: &SeriesFrame) -> Vec<SeriesFrame> {
    let n = base.values.len();
    if n < 4 {
        return Vec::new();
    }
    let mut out = Vec::new();

    let delta: Vec<f64> = base.values.windows(2).map(|w| w[1] - w[0]).collect();
    out.push(derived_frame(base, Derivation::Delta, delta));

    let pct: Vec<f64> = base
        .values
        .windows(2)
        .map(|w| {
            if w[0].abs() < 1e-9 {
                f64::NAN
            } else {
                (w[1] - w[0]) / w[0].abs() * 100.0
            }
        })
        .collect();
    out.push(derived_frame(base, Derivation::PctChange, pct));
    out
}

fn derived_frame(base: &SeriesFrame, derivation: Derivation, values: Vec<f64>) -> SeriesFrame {
    let offset = base.values.len() - values.len();
    let mut provenance = base.provenance.clone();
    provenance.derivations.push(derivation);
    SeriesFrame {
        labels: base.labels[offset..].to_vec(),
        values,
        counts: base.counts[offset..].to_vec(),
        provenance,
        impact: base.impact,
    }
}

/// Slice ÷ whole-table, per period.
fn share_of_total(base: &SeriesFrame, index: &DimensionIndex) -> Option<SeriesFrame> {
    let total_values: Vec<f64> = match &base.provenance.measure {
        MeasureRef::RowCount => index.total.row_counts.iter().map(|c| *c as f64).collect(),
        MeasureRef::Column(m) => match base.provenance.aggregation {
            Aggregation::Sum => index.total.sums.get(m)?.clone(),
            // Share of a mean is not meaningful volume-wise
            Aggregation::Mean | Aggregation::Count => return None,
        },
    };
    let values: Vec<f64> = base
        .values
        .iter()
        .zip(total_values.iter())
        .map(|(v, t)| if t.abs() < 1e-9 { f64::NAN } else { v / t })
        .collect();
    let mut provenance = base.provenance.clone();
    provenance.derivations.push(Derivation::ShareOfTotal);
    Some(SeriesFrame {
        labels: base.labels.clone(),
        values,
        counts: base.counts.clone(),
        provenance,
        impact: base.impact,
    })
}

/// Per dimension: each top value's per-period volume rank among its siblings.
fn rank_series(
    index: &DimensionIndex,
    granularity: &str,
    budget: &EnumerationBudget,
) -> Vec<SeriesFrame> {
    let n_periods = index.periods.len();
    let mut out = Vec::new();

    let mut by_dim: HashMap<&str, Vec<&SliceEntry>> = HashMap::new();
    for slice in &index.slices {
        by_dim
            .entry(slice.dimension.as_str())
            .or_default()
            .push(slice);
    }

    for (dim, slices) in by_dim {
        if slices.len() < 3 {
            continue;
        }
        // ranks[period][slice_idx]
        for (si, slice) in slices.iter().enumerate() {
            let impact = slice.agg.total_rows() as f64 / index.total_rows as f64;
            if impact < budget.min_impact {
                continue;
            }
            let mut ranks: Vec<f64> = Vec::with_capacity(n_periods);
            for p in 0..n_periods {
                let own = slice.agg.row_counts[p];
                let rank = 1 + slices
                    .iter()
                    .enumerate()
                    .filter(|(oi, other)| *oi != si && other.agg.row_counts[p] > own)
                    .count();
                ranks.push(rank as f64);
            }
            out.push(SeriesFrame {
                labels: index.periods.clone(),
                values: ranks,
                counts: slice.agg.row_counts.clone(),
                provenance: Provenance {
                    measure: MeasureRef::RowCount,
                    aggregation: Aggregation::Count,
                    filters: vec![FilterSpec {
                        column: dim.to_string(),
                        value: slice.value.clone(),
                    }],
                    derivations: vec![Derivation::RankAmongSiblings],
                    granularity: granularity.to_string(),
                },
                impact,
            });
        }
    }
    out
}

/// Top-value shares over the full range for one dimension × measure —
/// input to the TopDominance detector.
/// (value, share) pairs sorted descending + rows covered.
pub type DominanceShares = (Vec<(String, f64)>, usize);

pub fn dominance_shares(
    index: &DimensionIndex,
    dimension: &str,
    measure: &MeasureRef,
) -> Option<DominanceShares> {
    let slices: Vec<&SliceEntry> = index
        .slices
        .iter()
        .filter(|s| s.dimension == dimension)
        .collect();
    if slices.len() < 4 {
        return None;
    }
    let mut totals: Vec<(String, f64)> = slices
        .iter()
        .map(|s| {
            let total = match measure {
                MeasureRef::RowCount => s.agg.total_rows() as f64,
                MeasureRef::Column(m) => s.agg.sums.get(m).map_or(0.0, |sums| sums.iter().sum()),
            };
            (s.value.clone(), total)
        })
        .collect();
    let sum: f64 = totals.iter().map(|(_, v)| v.max(0.0)).sum();
    if sum <= 0.0 {
        return None;
    }
    for (_, v) in &mut totals {
        *v = v.max(0.0) / sum;
    }
    totals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let n_rows_covered = slices.iter().map(|s| s.agg.total_rows()).sum();
    Some((totals, n_rows_covered))
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss)]
mod tests {
    use super::*;

    fn make_cache() -> (ColumnCache, Vec<Option<String>>) {
        let mut numeric = HashMap::new();
        let mut dimension = HashMap::new();
        // 3 periods × 2 regions × 2 rows each
        let periods = ["p1", "p2", "p3"];
        let regions = ["EU", "NA"];
        let mut period_labels = Vec::new();
        let mut region_col = Vec::new();
        let mut revenue = Vec::new();
        for p in &periods {
            for r in &regions {
                for k in 0..2 {
                    period_labels.push(Some((*p).to_string()));
                    region_col.push((*r).to_string());
                    revenue.push(10.0 + f64::from(k));
                }
            }
        }
        numeric.insert("revenue".to_string(), revenue);
        dimension.insert("region".to_string(), region_col);
        (ColumnCache { numeric, dimension }, period_labels)
    }

    #[test]
    fn index_counts_rows_per_period() {
        let (cache, labels) = make_cache();
        let index = DimensionIndex::build(&cache, &labels, &EnumerationBudget::default());
        assert_eq!(index.periods, vec!["p1", "p2", "p3"]);
        assert_eq!(index.total.row_counts, vec![4, 4, 4]);
        assert_eq!(index.total_rows, 12);
        // Two region slices, each 2 rows per period
        assert_eq!(index.slices.len(), 2);
        for s in &index.slices {
            assert_eq!(s.agg.row_counts, vec![2, 2, 2]);
        }
    }

    #[test]
    fn enumeration_emits_rowcount_series() {
        let (cache, labels) = make_cache();
        let index = DimensionIndex::build(&cache, &labels, &EnumerationBudget::default());
        let (frames, stats) = enumerate_series(&index, "day", &EnumerationBudget::default());
        assert!(stats.series_emitted > 0);
        // Whole-table row count must exist
        let whole_count = frames.iter().find(|f| {
            f.provenance.measure == MeasureRef::RowCount
                && f.provenance.filters.is_empty()
                && f.provenance.derivations.is_empty()
        });
        assert!(whole_count.is_some(), "counts/period series must exist");
        assert_eq!(whole_count.unwrap().values, vec![4.0, 4.0, 4.0]);
        // Slice share-of-total must exist
        let share = frames.iter().find(|f| {
            f.provenance.derivations == vec![Derivation::ShareOfTotal]
                && !f.provenance.filters.is_empty()
        });
        assert!(share.is_some(), "share-of-total series must exist");
    }

    #[test]
    fn impact_is_antimonotonic_under_filtering() {
        // Property: every depth-2 frame's impact ≤ min of its parents' impacts.
        let mut numeric = HashMap::new();
        let mut dim = HashMap::new();
        let mut labels = Vec::new();
        let mut d1 = Vec::new();
        let mut d2 = Vec::new();
        let mut m = Vec::new();
        // Deterministic mix over 4 periods
        for i in 0..400_usize {
            labels.push(Some(format!("p{}", i % 4)));
            d1.push(format!("a{}", i % 3));
            d2.push(format!("b{}", (i / 3) % 4));
            m.push(i as f64);
        }
        numeric.insert("m".to_string(), m);
        dim.insert("d1".to_string(), d1);
        dim.insert("d2".to_string(), d2);
        let cache = ColumnCache {
            numeric,
            dimension: dim,
        };
        let budget = EnumerationBudget {
            min_impact: 0.0,
            ..EnumerationBudget::default()
        };
        let index = DimensionIndex::build(&cache, &labels, &budget);
        let (frames, _) = enumerate_series(&index, "day", &budget);

        let impact_of = |filters: &[FilterSpec]| -> Option<f64> {
            frames
                .iter()
                .find(|f| {
                    f.provenance.filters == filters
                        && f.provenance.measure == MeasureRef::RowCount
                        && f.provenance.derivations.is_empty()
                })
                .map(|f| f.impact)
        };

        for f in frames.iter().filter(|f| f.provenance.filters.len() == 2) {
            let child = f.impact;
            for parent_filter in &f.provenance.filters {
                let parent = impact_of(std::slice::from_ref(parent_filter))
                    .expect("parent slice must have been enumerated");
                assert!(
                    child <= parent + 1e-9,
                    "impact must be anti-monotonic: child {child} > parent {parent}"
                );
            }
        }
    }

    #[test]
    fn budget_caps_series_count() {
        let (cache, labels) = make_cache();
        let budget = EnumerationBudget {
            max_series: 3,
            ..EnumerationBudget::default()
        };
        let index = DimensionIndex::build(&cache, &labels, &budget);
        let (frames, stats) = enumerate_series(&index, "day", &budget);
        assert!(frames.len() <= 3);
        assert!(stats.series_dropped_by_budget > 0);
    }

    #[test]
    fn provenance_label_reads_like_a_recipe() {
        let p = Provenance {
            measure: MeasureRef::RowCount,
            aggregation: Aggregation::Count,
            filters: vec![FilterSpec {
                column: "region".to_string(),
                value: "EU".to_string(),
            }],
            derivations: vec![Derivation::ShareOfTotal],
            granularity: "day".to_string(),
        };
        assert_eq!(
            p.label(),
            "count of rows per day where region=EU, share of total"
        );
        assert_eq!(p.depth(), 3);
    }
}

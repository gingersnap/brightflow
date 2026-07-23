//! Derived-series insights pass: enumerate → detect → score → annotate.
//!
//! This is the core the research prescribed: instead of testing pre-existing
//! numeric columns row-by-row, materialize the series that carry stories
//! (counts/day, per-segment sums, shares, ranks) and run typed detectors with
//! honest nulls over them. Every finding carries provenance (how the series
//! was derived), a why-interesting sentence, a composition depth, and a
//! stable fingerprint.

use tracing::debug;

use crate::analysis::anomaly::detect_anomaly_in_series;
use crate::analysis::candidates::{
    dominance_shares, enumerate_series, Derivation, DimensionIndex, EnumerationBudget, MeasureRef,
    Provenance, SeriesFrame,
};
use crate::analysis::change_point::detect_change_point;
use crate::analysis::null_models::{
    change_point_null, point_null, rank_change_null, top1_null, trend_null, Significance,
};
use crate::analysis::tree::{AnalysisTree, AnalysisType, FilterStep, NodeData, ScoreBreakdown};
use crate::analysis::trend::detect_trend_in_series;
use crate::data::config::TimeGranularity;

use super::cache::ColumnCache;
use super::payloads::{build_anomaly_period_series, build_trend_series_data};
use super::AnalysisEngine;

/// Depth-1 (bare aggregate, no filter/derivation) findings must clear a
/// stricter significance bar — "the total has a trend" is trivially true of
/// most growing datasets.
const DEPTH1_MIN_SIGNIFICANCE: f64 = 0.8;
/// Everything else clears the standard bar.
const MIN_SIGNIFICANCE: f64 = 0.5;
/// Rank must move at least this many places to be a story.
const MIN_RANK_JUMP: usize = 2;

pub(super) fn granularity_name(g: TimeGranularity) -> &'static str {
    match g {
        TimeGranularity::Day => "day",
        TimeGranularity::Week => "week",
        TimeGranularity::Month => "month",
        TimeGranularity::Quarter => "quarter",
        TimeGranularity::Year => "year",
    }
}

impl AnalysisEngine {
    /// Run the derived-series pass, adding roots to `tree`.
    /// Returns the number of candidate series examined.
    pub(super) fn run_derived_series_pass(
        &self,
        cache: &ColumnCache,
        period_labels: &[Option<String>],
        granularity: TimeGranularity,
        tree: &mut AnalysisTree,
    ) -> usize {
        let budget = EnumerationBudget::default();
        let gran = granularity_name(granularity);
        let index = DimensionIndex::build(cache, period_labels, &budget);
        let (frames, stats) = enumerate_series(&index, gran, &budget);
        debug!(
            "derived-series pass: {} series (pruned {} slices by impact, dropped {} by budget)",
            stats.series_emitted, stats.slices_pruned_by_impact, stats.series_dropped_by_budget
        );

        let examined = frames.len();
        for frame in &frames {
            self.detect_on_frame(frame, tree);
        }

        // Cross-slice dominance: per dimension × {row volume, measure sums}
        let mut dominance_measures: Vec<MeasureRef> = vec![MeasureRef::RowCount];
        for m in cache.numeric.keys() {
            dominance_measures.push(MeasureRef::Column(m.clone()));
        }
        let dims: std::collections::HashSet<String> = cache.dimension.keys().cloned().collect();
        for dim in dims {
            for measure in &dominance_measures {
                self.detect_dominance(&index, &dim, measure, gran, tree);
            }
        }

        examined
    }

    /// Run the typed detectors appropriate for one frame.
    fn detect_on_frame(&self, frame: &SeriesFrame, tree: &mut AnalysisTree) {
        if frame
            .provenance
            .derivations
            .contains(&Derivation::RankAmongSiblings)
        {
            self.detect_rank_change(frame, tree);
            return;
        }

        // Drop leading non-finite points (Δ/%Δ/share warm-up); skip frames
        // with interior gaps.
        let Some((labels, values)) = finite_view(frame) else {
            return;
        };
        if values.len() < 4 {
            return;
        }

        self.detect_frame_trend(frame, &labels, &values, tree);
        self.detect_frame_change_point(frame, &labels, &values, tree);
        self.detect_frame_last_point(frame, &labels, &values, tree);
    }

    fn detect_frame_trend(
        &self,
        frame: &SeriesFrame,
        labels: &[String],
        values: &[f64],
        tree: &mut AnalysisTree,
    ) {
        let Some(trend) = detect_trend_in_series(&display_name(&frame.provenance), values) else {
            return;
        };
        let sig = trend_null(values);
        let Some(breakdown) = self.frame_breakdown(frame, sig, &frame.provenance.measure) else {
            return;
        };
        let analysis = AnalysisType::Trend {
            column: trend.column,
            direction: trend.direction,
            slope: trend.slope,
            r_squared: trend.r_squared,
            p_value: sig.p_value,
        };
        let description = format!(
            "Trend in {}: slope={:.4}, R²={:.3} (p={:.4})",
            frame.provenance.label(),
            trend.slope,
            trend.r_squared,
            sig.p_value
        );
        let why = why_sentence(
            frame,
            sig,
            &format!(
                "a flat series would show this pattern {:.1}% of the time",
                sig.p_value * 100.0
            ),
        );
        let data = Some(build_trend_series_data(labels, values, trend.slope));
        self.add_frame_root(
            tree,
            frame,
            "trend",
            analysis,
            breakdown,
            description,
            why,
            data,
        );
    }

    fn detect_frame_change_point(
        &self,
        frame: &SeriesFrame,
        labels: &[String],
        values: &[f64],
        tree: &mut AnalysisTree,
    ) {
        let pairs: Vec<(String, f64)> =
            labels.iter().cloned().zip(values.iter().copied()).collect();
        let Some(cp) = detect_change_point(&display_name(&frame.provenance), &pairs) else {
            return;
        };
        let Some(cp_idx) = labels.iter().position(|l| l == &cp.period) else {
            return;
        };
        let sig = change_point_null(values, cp_idx);
        let Some(breakdown) = self.frame_breakdown(frame, sig, &frame.provenance.measure) else {
            return;
        };
        let analysis = AnalysisType::ChangePoint {
            column: cp.column,
            period: cp.period.clone(),
            before_mean: cp.before_mean,
            after_mean: cp.after_mean,
            cusum: cp.cusum,
            p_value: sig.p_value,
        };
        let description = format!(
            "Change-point in {} at {}: {:.2} → {:.2} (p={:.4})",
            frame.provenance.label(),
            cp.period,
            cp.before_mean,
            cp.after_mean,
            sig.p_value
        );
        let why = why_sentence(
            frame,
            sig,
            &format!(
                "a stable series would produce a step this clean {:.1}% of the time",
                sig.p_value * 100.0
            ),
        );
        let fit: Vec<f64> = (0..values.len())
            .map(|i| {
                if i <= cp_idx {
                    cp.before_mean
                } else {
                    cp.after_mean
                }
            })
            .collect();
        let data = Some(NodeData::SeriesWithFit {
            labels: labels.to_vec(),
            values: values.to_vec(),
            fit,
            y_label: None,
        });
        self.add_frame_root(
            tree,
            frame,
            "change_point",
            analysis,
            breakdown,
            description,
            why,
            data,
        );
    }

    fn detect_frame_last_point(
        &self,
        frame: &SeriesFrame,
        labels: &[String],
        values: &[f64],
        tree: &mut AnalysisTree,
    ) {
        let Some(anomaly) = detect_anomaly_in_series(&display_name(&frame.provenance), values)
        else {
            return;
        };
        if anomaly.z_score.abs() <= self.z_threshold {
            return;
        }
        // Significance from the detrended null so steady growth is boring.
        let sig = point_null(values);
        let Some(breakdown) = self.frame_breakdown(frame, sig, &frame.provenance.measure) else {
            return;
        };
        let analysis = AnalysisType::Anomaly {
            column: anomaly.column.clone(),
            value: anomaly.value,
            mean: anomaly.mean,
            std_dev: anomaly.std_dev,
            z_score: anomaly.z_score,
        };
        let last_label = labels.last().cloned().unwrap_or_default();
        let description = format!(
            "Latest {} of {}: {:.2} vs historical {:.2} (z={:.1}, detrended p={:.4})",
            last_label,
            frame.provenance.label(),
            anomaly.value,
            anomaly.mean,
            anomaly.z_score,
            sig.p_value
        );
        let why = why_sentence(
            frame,
            sig,
            &format!(
                "even accounting for its trend, history would produce a point this extreme {:.1}% of the time",
                sig.p_value * 100.0
            ),
        );
        let data = Some(build_anomaly_period_series(
            labels,
            values,
            anomaly.mean,
            anomaly.std_dev,
        ));
        self.add_frame_root(
            tree,
            frame,
            "last_point",
            analysis,
            breakdown,
            description,
            why,
            data,
        );
    }

    fn detect_rank_change(&self, frame: &SeriesFrame, tree: &mut AnalysisTree) {
        let n = frame.values.len();
        if n < 6 {
            return;
        }
        let window = (n / 3).clamp(2, 4);
        let first: f64 = frame.values[..window].iter().sum::<f64>() / window as f64;
        let last: f64 = frame.values[n - window..].iter().sum::<f64>() / window as f64;
        let previous_rank = first.round().max(1.0) as usize;
        let new_rank = last.round().max(1.0) as usize;
        let jump = previous_rank.abs_diff(new_rank);
        if jump < MIN_RANK_JUMP {
            return;
        }
        let n_siblings = frame
            .values
            .iter()
            .fold(0.0_f64, |a, b| a.max(*b))
            .round()
            .max(2.0) as usize;
        let sig = rank_change_null(jump, n_siblings, n);
        let Some(breakdown) = self.frame_breakdown(frame, sig, &frame.provenance.measure) else {
            return;
        };
        let (dimension, value) = frame
            .provenance
            .filters
            .first()
            .map(|f| (f.column.clone(), f.value.clone()))
            .unwrap_or_default();
        let analysis = AnalysisType::RankChange {
            measure: frame.provenance.measure.name().to_string(),
            dimension: dimension.clone(),
            value: value.clone(),
            previous_rank,
            new_rank,
            n_siblings,
            p_value: sig.p_value,
        };
        let description = format!(
            "{dimension}=\"{value}\" moved from rank {previous_rank} to {new_rank} of {n_siblings} by {} volume (p={:.4})",
            frame.provenance.granularity, sig.p_value
        );
        let why = why_sentence(
            frame,
            sig,
            &format!("a {jump}-place move in a field of {n_siblings} peers"),
        );
        let data = Some(NodeData::Series {
            labels: frame.labels.clone(),
            values: frame.values.clone(),
            band_low: None,
            band_high: None,
            marker_index: Some(n - 1),
            y_label: Some("Rank among peers".to_string()),
        });
        self.add_frame_root(
            tree,
            frame,
            "rank_change",
            analysis,
            breakdown,
            description,
            why,
            data,
        );
    }

    pub(super) fn detect_dominance(
        &self,
        index: &DimensionIndex,
        dimension: &str,
        measure: &MeasureRef,
        gran: &str,
        tree: &mut AnalysisTree,
    ) {
        let Some((shares, _covered)) = dominance_shares(index, dimension, measure) else {
            return;
        };
        let share_values: Vec<f64> = shares.iter().map(|(_, s)| *s).collect();
        let sig = top1_null(&share_values);
        let (leader, leader_share) = shares[0].clone();
        // Expected leader share from the power-law fit ≈ observed / e^(z·σ);
        // recompute directly for display: refit on ranks 2..k.
        let expected = expected_top_share(&share_values);

        let provenance = Provenance {
            measure: measure.clone(),
            aggregation: match measure {
                MeasureRef::RowCount => crate::analysis::candidates::Aggregation::Count,
                MeasureRef::Column(_) => crate::analysis::candidates::Aggregation::Sum,
            },
            filters: vec![crate::analysis::candidates::FilterSpec {
                column: dimension.to_string(),
                value: leader.clone(),
            }],
            derivations: vec![Derivation::ShareOfTotal],
            granularity: gran.to_string(),
        };
        let frame = SeriesFrame {
            labels: Vec::new(),
            values: Vec::new(),
            counts: Vec::new(),
            provenance,
            impact: leader_share,
        };
        let Some(breakdown) = self.frame_breakdown(&frame, sig, measure) else {
            return;
        };
        let analysis = AnalysisType::TopDominance {
            measure: measure.name().to_string(),
            dimension: dimension.to_string(),
            value: leader.clone(),
            share: leader_share,
            expected_share: expected,
            n_values: shares.len(),
            p_value: sig.p_value,
        };
        let description = format!(
            "{dimension}=\"{leader}\" holds {:.1}% of {} — the rank distribution predicts {:.1}% (p={:.4})",
            leader_share * 100.0,
            measure.name(),
            expected * 100.0,
            sig.p_value
        );
        let why = format!(
            "the leader holds {:.0}% of the volume where its own tail predicts {:.0}%",
            leader_share * 100.0,
            expected * 100.0
        );
        let data = Some(NodeData::SegmentBars {
            labels: shares.iter().map(|(v, _)| v.clone()).collect(),
            values: shares.iter().map(|(_, s)| s * 100.0).collect(),
            contributions_pct: shares.iter().map(|(_, s)| s * 100.0).collect(),
            value_label: Some(format!("Share of {} (%)", measure.name())),
        });
        self.add_frame_root(
            tree,
            &frame,
            "top_dominance",
            analysis,
            breakdown,
            description,
            why,
            data,
        );
    }

    /// Score a frame finding; None = fails the floor.
    fn frame_breakdown(
        &self,
        frame: &SeriesFrame,
        sig: Significance,
        measure: &MeasureRef,
    ) -> Option<ScoreBreakdown> {
        let min_sig = if frame.provenance.depth() <= 1 {
            DEPTH1_MIN_SIGNIFICANCE
        } else {
            MIN_SIGNIFICANCE
        };
        if sig.score < min_sig {
            return None;
        }
        let kpi_boost = match measure {
            MeasureRef::Column(c) if self.scoring_ctx.kpi_columns.contains(c) => 1.5,
            _ => 1.0,
        };
        Some(ScoreBreakdown {
            significance: sig.score,
            impact: frame.impact.clamp(0.0, 1.0),
            novelty: 1.0,
            kpi_boost,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::unused_self)]
    fn add_frame_root(
        &self,
        tree: &mut AnalysisTree,
        frame: &SeriesFrame,
        detector: &str,
        analysis: AnalysisType,
        breakdown: ScoreBreakdown,
        description: String,
        why: String,
        data: Option<NodeData>,
    ) {
        let score = crate::analysis::scoring::total(&breakdown);
        let node_id = tree.add_root_full(analysis, score, breakdown, description, data);
        tree.set_insight_meta(
            node_id,
            why,
            frame.provenance.steps(),
            frame.provenance.depth(),
            frame.provenance.fingerprint(detector),
        );
        let chain: Vec<FilterStep> = frame
            .provenance
            .filters
            .iter()
            .map(|f| FilterStep {
                column: f.column.clone(),
                op: "eq".to_string(),
                value: f.value.clone(),
            })
            .collect();
        if !chain.is_empty() {
            tree.set_filter_chain(node_id, chain);
        }
    }
}

/// Short display name for a derived series, used as the `column` of legacy
/// analysis variants: `rows · region=EU · share of total`.
fn display_name(p: &Provenance) -> String {
    let mut parts: Vec<String> = Vec::new();
    match (&p.measure, p.aggregation) {
        (MeasureRef::RowCount, _) => parts.push("rows".to_string()),
        (MeasureRef::Column(c), agg) => parts.push(format!("{c} ({})", agg.name())),
    }
    for f in &p.filters {
        parts.push(format!("{}={}", f.column, f.value));
    }
    for d in &p.derivations {
        parts.push(d.human().to_string());
    }
    parts.join(" · ")
}

/// Trim leading non-finite points; reject frames with interior gaps.
fn finite_view(frame: &SeriesFrame) -> Option<(Vec<String>, Vec<f64>)> {
    let first_finite = frame.values.iter().position(|v| v.is_finite())?;
    let labels = frame.labels[first_finite..].to_vec();
    let values = frame.values[first_finite..].to_vec();
    if values.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some((labels, values))
}

/// One "why is this interesting" sentence: impact + null framing
/// (+ low-confidence caveat).
fn why_sentence(frame: &SeriesFrame, sig: Significance, null_clause: &str) -> String {
    use std::fmt::Write as _;
    let mut s = format!(
        "affects {:.0}% of rows; {null_clause}",
        frame.impact * 100.0
    );
    if sig.low_confidence {
        let _ = write!(s, " (low confidence: only {} periods)", frame.values.len());
    }
    s
}

/// Power-law prediction for the leader's share, refit on ranks 2..k.
fn expected_top_share(shares: &[f64]) -> f64 {
    let k = shares.len();
    if k < 4 {
        return shares.first().copied().unwrap_or(0.0);
    }
    let xs: Vec<f64> = (2..=k).map(|r| (r as f64).ln()).collect();
    let ys: Vec<f64> = shares[1..].iter().map(|s| s.max(1e-12).ln()).collect();
    match crate::stats::significance::linear_regression(&xs, &ys) {
        Some((_slope, intercept, _)) => intercept.exp().clamp(0.0, 1.0),
        None => shares[0],
    }
}

//! Calibrated interestingness scoring.
//!
//! Each finding gets two normalized components:
//! - **Significance**: 1 - p_value where the p-value comes from a
//!   type-specific "boring baseline" null (uniform multinomial for
//!   concentration, binomial co-occurrence for outlier clusters, baseline
//!   churn for membership changes, standard tests elsewhere).
//! - **Effect size**: per-type, normalized to [0, 1] using effect-size
//!   conventions (Cohen's d, |r|, R², z/5, etc.)
//!
//! The final score is a product:
//!   score = significance * sqrt(effect_size) * kpi_boost
//!
//! The product form means a finding must be BOTH statistically real AND
//! materially large to rank — a weighted sum lets one component compensate
//! for the other, which is how trivia used to surface.

// Match arms over `AnalysisType` are intentionally exhaustive per variant for
// readability and so that adding a new variant raises a compile error rather
// than silently falling into a default. Suppress the related nursery lints.
#![allow(clippy::match_same_arms)]

use std::collections::{HashMap, HashSet};

use statrs::distribution::{Binomial, ChiSquared, ContinuousCDF, DiscreteCDF};

use crate::analysis::candidates::{Aggregation, FilterSpec, MeasureRef};
use crate::analysis::polarity::direction_of_analysis;
use crate::analysis::select::measure_of_analysis;
use crate::analysis::tree::{AnalysisType, ScoreBreakdown, TrendDirection};
use crate::data::config::Polarity;

// ─── Calibration constants ────────────────────────────────────────────────────

/// Multiplier applied when a finding's measure is flagged as KPI
const KPI_MULTIPLIER: f64 = 1.5;

/// Effect-size floor — findings below this are dropped entirely
const DEFAULT_MIN_EFFECT: f64 = 0.05;

/// Significance floor — findings whose null can't be rejected at this level
/// are dropped regardless of effect size
const MIN_SIGNIFICANCE: f64 = 0.5;

/// Multiplier for a finding that is bad news under the measure's declared
/// polarity. Modest on purpose: it orders two otherwise equal findings, it
/// does not lift noise over signal.
pub const BAD_NEWS_MULTIPLIER: f64 = 1.25;

/// Per-column probability of a same-direction period outlier under H0.
/// One-sided tail at the default z-threshold of 2.0 (Φ(-2) ≈ 0.0228).
const OUTLIER_NULL_P: f64 = 0.0228;

/// Baseline per-member churn probability between adjacent periods under H0.
/// Membership changes below this base rate are considered ordinary turnover.
const MEMBERSHIP_NULL_CHURN: f64 = 0.05;

// ─── Scoring context ──────────────────────────────────────────────────────────

/// Context passed to scoring — carries dataset-wide info needed for KPI
/// flagging, polarity and floor overrides. Fields can be empty (no-ops) for
/// the basic case.
#[derive(Debug, Clone, Default)]
pub struct ScoringContext {
    /// Columns flagged as KPIs (from the semantic layer)
    pub kpi_columns: HashSet<String>,
    /// Declared measure polarity, non-neutral entries only.
    pub polarity: HashMap<String, Polarity>,
    /// Declared metrics: a (column, aggregation, filters) someone named. A
    /// derived series that matches one is a KPI series even when its bare
    /// column is not a KPI.
    pub metrics: Vec<MetricSpec>,
    /// Min effect size (override DEFAULT_MIN_EFFECT)
    pub min_effect_size: Option<f64>,
}

/// A declared metric in the engine's own terms.
///
/// The column, the candidate aggregation it corresponds to, and its equality
/// filters. `None` where the declaration used an aggregation or a filter the
/// enumeration never produces, so it can never match a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricSpec {
    pub name: String,
    pub column: String,
    pub aggregation: Option<Aggregation>,
    pub filters: Option<Vec<FilterSpec>>,
    pub is_kpi: bool,
    pub polarity: Polarity,
}

impl MetricSpec {
    /// From the contract's metric extension. Sum, avg and count map onto
    /// the enumeration's aggregations; only `=` filters with a scalar value
    /// are expressible.
    pub fn from_ext(name: &str, ext: &brightflow_types::MetricExt) -> Self {
        use brightflow_types::{Aggregation as A, FilterOp};
        let aggregation = match ext.expr.aggregation {
            A::Sum => Some(Aggregation::Sum),
            A::Avg => Some(Aggregation::Mean),
            A::Count => Some(Aggregation::Count),
            _ => None,
        };
        let filters = ext
            .expr
            .filters
            .iter()
            .map(|f| match (&f.op, &f.value) {
                (FilterOp::Eq, Some(serde_json::Value::String(v))) => Some(FilterSpec {
                    column: f.column.clone(),
                    value: v.clone(),
                }),
                (
                    FilterOp::Eq,
                    Some(v @ (serde_json::Value::Number(_) | serde_json::Value::Bool(_))),
                ) => Some(FilterSpec {
                    column: f.column.clone(),
                    value: v.to_string(),
                }),
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        Self {
            name: name.to_string(),
            column: ext.expr.column.clone(),
            aggregation,
            filters,
            is_kpi: ext.is_kpi.unwrap_or(false),
            polarity: ext.polarity.unwrap_or_default(),
        }
    }

    /// Whether a derived series is this metric: same column and
    /// aggregation, and every one of the metric's filters among the series'
    /// filters (the series may slice further).
    pub fn matches(
        &self,
        measure: &MeasureRef,
        aggregation: Aggregation,
        filters: &[FilterSpec],
    ) -> bool {
        let (Some(agg), Some(own)) = (self.aggregation, &self.filters) else {
            return false;
        };
        let column_matches = match measure {
            MeasureRef::Column(c) => *c == self.column,
            MeasureRef::RowCount => agg == Aggregation::Count,
        };
        column_matches && agg == aggregation && own.iter().all(|f| filters.contains(f))
    }
}

impl ScoringContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_kpis(mut self, kpis: HashSet<String>) -> Self {
        self.kpi_columns = kpis;
        self
    }

    pub fn with_polarity(mut self, polarity: HashMap<String, Polarity>) -> Self {
        self.polarity = polarity;
        self
    }

    pub fn with_metrics(mut self, metrics: Vec<MetricSpec>) -> Self {
        self.metrics = metrics;
        self
    }

    /// The KPI multiplier for a derived series: its column is a KPI, or a
    /// KPI metric names exactly this series.
    pub fn series_kpi_boost(
        &self,
        measure: &MeasureRef,
        aggregation: Aggregation,
        filters: &[FilterSpec],
    ) -> f64 {
        let column_kpi = matches!(measure, MeasureRef::Column(c) if self.kpi_columns.contains(c));
        let metric_kpi = self
            .metrics
            .iter()
            .any(|m| m.is_kpi && m.matches(measure, aggregation, filters));
        if column_kpi || metric_kpi {
            KPI_MULTIPLIER
        } else {
            1.0
        }
    }

    pub fn with_min_effect(mut self, min: f64) -> Self {
        self.min_effect_size = Some(min);
        self
    }

    fn min_effect(&self) -> f64 {
        self.min_effect_size.unwrap_or(DEFAULT_MIN_EFFECT)
    }
}

// ─── Public scoring API ───────────────────────────────────────────────────────

/// Score an analysis finding. Returns the breakdown — caller multiplies the
/// components into a final scalar via `total()`.
pub fn score(analysis: &AnalysisType, ctx: &ScoringContext) -> ScoreBreakdown {
    let sig = significance_component(analysis);
    // For legacy detectors the normalized effect size stands in for impact;
    // the derived-series pipeline computes true volume impact directly.
    let impact = effect_size_component(analysis);
    let kpi_boost = kpi_boost_for(analysis, ctx);
    let polarity_boost = polarity_boost_for(analysis, ctx);

    ScoreBreakdown {
        significance: sig,
        impact,
        // Novelty defaults to 1.0 (never seen) until insight history lands (1C).
        novelty: 1.0,
        kpi_boost,
        polarity_boost,
    }
}

/// Combine the breakdown into a single scalar.
///
/// Product form: a finding must be both significant and material; sqrt on
/// impact softens its dominance; novelty modulates rather than vetoes (a
/// fully known story loses half its score).
pub fn total(b: &ScoreBreakdown) -> f64 {
    b.significance
        * b.impact.max(0.0).sqrt()
        * 0.5f64.mul_add(b.novelty.clamp(0.0, 1.0), 0.5)
        * b.kpi_boost
        * b.polarity_boost
}

/// Returns true if a finding should be kept: impact above the floor AND the
/// type-specific null rejected with at least MIN_SIGNIFICANCE confidence.
pub fn passes_floor(b: &ScoreBreakdown, ctx: &ScoringContext) -> bool {
    b.impact >= ctx.min_effect() && b.significance >= MIN_SIGNIFICANCE
}

// ─── Component computations ───────────────────────────────────────────────────

fn significance_component(a: &AnalysisType) -> f64 {
    let p = match a {
        AnalysisType::Anomaly { z_score, .. } => {
            // No explicit p-value on anomalies — derive from |z| via normal CDF approx
            two_tailed_p_from_z(*z_score)
        },
        AnalysisType::Segment { p_value, .. } => *p_value,
        AnalysisType::Correlation { p_value, .. } => *p_value,
        AnalysisType::Trend { p_value, .. } => *p_value,
        AnalysisType::PeriodComparison { p_value, .. } => *p_value,
        AnalysisType::PeriodAnomaly { p_value, .. } => *p_value,
        AnalysisType::Seasonality { p_value, .. } => *p_value,
        AnalysisType::ForecastDeviation { p_value, .. } => *p_value,
        AnalysisType::OutlierCluster {
            cluster_size,
            columns_tested,
            n_periods,
            ..
        } => p_outlier_cluster(*cluster_size, *columns_tested, *n_periods),
        AnalysisType::Concentration {
            hhi,
            n_segments,
            n_rows,
            ..
        } => p_concentration_vs_uniform(*hhi, *n_segments, *n_rows),
        AnalysisType::DistributionShift { p_value, .. } => *p_value,
        AnalysisType::MembershipChange {
            added_count,
            removed_count,
            prev_size,
            ..
        } => p_membership_churn(*added_count, *removed_count, *prev_size),
        AnalysisType::ChangePoint { p_value, .. } => *p_value,
        AnalysisType::RankChange { p_value, .. } => *p_value,
        AnalysisType::TopDominance { p_value, .. } => *p_value,
    };
    (1.0 - p.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

/// P-value of the observed HHI against a uniform-multinomial null.
///
/// Uses the identity X² = n·k·(HHI − 1/k) where X² is the chi-square
/// goodness-of-fit statistic against equal shares, with df = k − 1.
/// The metric mass is treated as pseudo-counts distributed over `n_rows`
/// observations — an approximation, but a directionally honest one: uniform
/// shares now score p ≈ 1 instead of the old inverted `1 − HHI`.
fn p_concentration_vs_uniform(hhi: f64, n_segments: usize, n_rows: usize) -> f64 {
    if n_segments < 2 || n_rows < n_segments {
        return 1.0;
    }
    let k = n_segments as f64;
    let n = n_rows as f64;
    let x2 = (n * k * (hhi - 1.0 / k)).max(0.0);
    let df = k - 1.0;
    match ChiSquared::new(df) {
        Ok(dist) => (1.0 - dist.cdf(x2)).clamp(0.0, 1.0),
        Err(_) => 1.0,
    }
}

/// P-value that ≥ cluster_size of columns_tested columns show a
/// same-direction period outlier by chance (binomial tail), Bonferroni-
/// corrected for scanning every (period × direction) combination.
fn p_outlier_cluster(cluster_size: usize, columns_tested: usize, n_periods: usize) -> f64 {
    if cluster_size < 2 || columns_tested < cluster_size {
        return 1.0;
    }
    let Ok(dist) = Binomial::new(OUTLIER_NULL_P, columns_tested as u64) else {
        return 1.0;
    };
    // P(X >= cluster_size) = 1 - CDF(cluster_size - 1)
    let tail = 1.0 - dist.cdf(cluster_size as u64 - 1);
    let comparisons = (n_periods.max(1) * 2) as f64;
    (tail * comparisons).clamp(0.0, 1.0)
}

/// P-value of the observed membership churn against a baseline churn rate.
/// Exposure is the union of members seen in either period.
fn p_membership_churn(added: usize, removed: usize, prev_size: usize) -> f64 {
    let churn = added + removed;
    let exposure = prev_size + added; // union of prev members and new arrivals
    if churn == 0 || exposure == 0 {
        return 1.0;
    }
    let Ok(dist) = Binomial::new(MEMBERSHIP_NULL_CHURN, exposure as u64) else {
        return 1.0;
    };
    (1.0 - dist.cdf(churn.saturating_sub(1) as u64)).clamp(0.0, 1.0)
}

fn effect_size_component(a: &AnalysisType) -> f64 {
    match a {
        AnalysisType::Anomaly { z_score, .. } => clamp_unit(z_score.abs() / 5.0),
        AnalysisType::Segment {
            contribution_pct,
            change_percent,
            ..
        } => {
            // Use whichever is larger: contribution_pct (period) or change_percent (non-period)
            let by_contrib = contribution_pct.abs() / 100.0;
            let by_change = change_percent.abs() / 200.0;
            clamp_unit(by_contrib.max(by_change))
        },
        AnalysisType::Correlation { r_value, .. } => clamp_unit(r_value.abs()),
        AnalysisType::Trend {
            r_squared,
            slope,
            direction,
            ..
        } => {
            // Trend strength = R² * sign-of-slope normalization; small slopes still
            // weighted if R² is high (clear pattern even if shallow)
            let dir_sign = match direction {
                TrendDirection::Increasing => 1.0,
                TrendDirection::Decreasing => 1.0,
            };
            let slope_norm = (slope.abs() * 10.0).tanh();
            clamp_unit((r_squared * 0.7 + slope_norm * 0.3) * dir_sign)
        },
        AnalysisType::PeriodComparison { change_percent, .. } => {
            clamp_unit(change_percent.abs() / 100.0)
        },
        AnalysisType::PeriodAnomaly { change_percent, .. } => {
            clamp_unit(change_percent.abs() / 100.0)
        },
        AnalysisType::Seasonality {
            autocorrelation, ..
        } => clamp_unit(autocorrelation.abs()),
        AnalysisType::ForecastDeviation {
            deviation_percent, ..
        } => clamp_unit(deviation_percent.abs() / 100.0),
        AnalysisType::OutlierCluster {
            cluster_size,
            columns_tested,
            ..
        } => {
            // Share of the measure space moving together
            if *columns_tested == 0 {
                0.0
            } else {
                clamp_unit(*cluster_size as f64 / *columns_tested as f64)
            }
        },
        AnalysisType::Concentration { hhi, top_share, .. } => {
            // Concentration is interesting when HHI is high OR top_share is dominant
            clamp_unit(hhi.max(*top_share / 100.0))
        },
        AnalysisType::DistributionShift { ks_statistic, .. } => clamp_unit(*ks_statistic),
        AnalysisType::MembershipChange {
            added_count,
            removed_count,
            prev_size,
            ..
        } => {
            // Churn as a share of the membership, not an absolute count
            let exposure = (prev_size + added_count).max(1);
            clamp_unit((*added_count + *removed_count) as f64 / exposure as f64)
        },
        AnalysisType::ChangePoint {
            before_mean,
            after_mean,
            ..
        } => {
            // Effect = relative change at the step
            let denom = before_mean.abs().max(1e-6);
            clamp_unit((after_mean - before_mean).abs() / denom)
        },
        AnalysisType::RankChange {
            previous_rank,
            new_rank,
            n_siblings,
            ..
        } => {
            let jump = previous_rank.abs_diff(*new_rank) as f64;
            clamp_unit(jump / (*n_siblings).max(1) as f64)
        },
        AnalysisType::TopDominance { share, .. } => clamp_unit(*share),
    }
}

/// Multiply a finding's score when it points the bad way on a measure with a
/// declared polarity: a drop in revenue, a rise in churn.
///
/// Good news is not penalised — a surprising rise is as worth surfacing as a
/// fall — and a measure with no declared polarity is untouched, so a table
/// nobody has described ranks exactly as before. Polarity now comes from the
/// same layered rows as everything else (a connector declares it, a person
/// overrides it), which is what makes it safe to rank on.
pub fn polarity_boost_for(a: &AnalysisType, ctx: &ScoringContext) -> f64 {
    if ctx.polarity.is_empty() {
        return 1.0;
    }
    let Some(up) = direction_of_analysis(a) else {
        return 1.0;
    };
    let measure = measure_of_analysis(a);
    match ctx.polarity.get(&measure) {
        Some(Polarity::HigherIsBetter) if !up => BAD_NEWS_MULTIPLIER,
        Some(Polarity::LowerIsBetter) if up => BAD_NEWS_MULTIPLIER,
        _ => 1.0,
    }
}

/// Multiply a finding's score when it lands on a column the user marked as a KPI.
fn kpi_boost_for(a: &AnalysisType, ctx: &ScoringContext) -> f64 {
    if ctx.kpi_columns.is_empty() {
        return 1.0;
    }
    let column_opt = match a {
        AnalysisType::Anomaly { column, .. } => Some(column.as_str()),
        AnalysisType::Segment { target_column, .. } => Some(target_column.as_str()),
        AnalysisType::Correlation { column_a, .. } => Some(column_a.as_str()),
        AnalysisType::Trend { column, .. } => Some(column.as_str()),
        AnalysisType::PeriodComparison { column, .. } => Some(column.as_str()),
        AnalysisType::PeriodAnomaly { column, .. } => Some(column.as_str()),
        AnalysisType::Seasonality { column, .. } => Some(column.as_str()),
        AnalysisType::ForecastDeviation { column, .. } => Some(column.as_str()),
        AnalysisType::Concentration { column, .. } => Some(column.as_str()),
        AnalysisType::DistributionShift { column, .. } => Some(column.as_str()),
        AnalysisType::ChangePoint { column, .. } => Some(column.as_str()),
        AnalysisType::RankChange { measure, .. } => Some(measure.as_str()),
        AnalysisType::TopDominance { measure, .. } => Some(measure.as_str()),
        AnalysisType::MembershipChange { .. } => None,
        AnalysisType::OutlierCluster { columns, .. } => {
            // KPI boost if any of the cluster's columns is a KPI
            if columns.iter().any(|c| ctx.kpi_columns.contains(c.as_str())) {
                return KPI_MULTIPLIER;
            }
            None
        },
    };
    if let Some(col) = column_opt {
        if ctx.kpi_columns.contains(col) {
            return KPI_MULTIPLIER;
        }
    }
    1.0
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn clamp_unit(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// Approximate two-tailed p-value from a z-score via the standard normal CDF.
/// Uses Abramowitz & Stegun formula 26.2.17 — accuracy ~1.5e-7.
fn two_tailed_p_from_z(z: f64) -> f64 {
    let abs_z = z.abs();
    if abs_z > 8.0 {
        return 0.0;
    }
    // Standard normal PDF
    let pdf = (-0.5 * abs_z * abs_z).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let t = 1.0 / 0.231_641_9_f64.mul_add(abs_z, 1.0);
    let poly = 0.319_381_530
        + t * (-0.356_563_782 + t * (1.781_477_937 + t * (-1.821_255_978 + t * 1.330_274_429)));
    let cdf_upper = pdf * t * poly;
    (2.0 * cdf_upper).clamp(0.0, 1.0)
}

#[cfg(test)]
#[expect(
    clippy::shadow_unrelated,
    clippy::suboptimal_flops,
    clippy::redundant_clone,
    reason = "explicit clones keep test fixtures independent; sequential test cases reuse binding names; test math is written for readability, not FLOP count"
)]
mod tests {
    use super::*;

    fn anomaly(z: f64) -> AnalysisType {
        AnalysisType::Anomaly {
            column: "x".to_string(),
            value: 0.0,
            mean: 0.0,
            std_dev: 1.0,
            z_score: z,
        }
    }

    fn concentration(hhi: f64, top_share: f64, n_segments: usize, n_rows: usize) -> AnalysisType {
        AnalysisType::Concentration {
            column: "revenue".to_string(),
            segment_column: "region".to_string(),
            hhi,
            top_n: 3,
            top_share,
            hhi_delta: None,
            n_segments,
            n_rows,
        }
    }

    #[test]
    fn higher_z_yields_higher_score() {
        let ctx = ScoringContext::new();
        let s1 = total(&score(&anomaly(2.0), &ctx));
        let s2 = total(&score(&anomaly(4.0), &ctx));
        assert!(s2 > s1, "score should increase with |z|: s1={s1}, s2={s2}");
    }

    #[test]
    fn kpi_boost_doubles_score() {
        let ctx_no = ScoringContext::new();
        let mut kpis = HashSet::new();
        kpis.insert("x".to_string());
        let ctx_kpi = ScoringContext::new().with_kpis(kpis);
        let s_no = total(&score(&anomaly(3.0), &ctx_no));
        let s_kpi = total(&score(&anomaly(3.0), &ctx_kpi));
        assert!((s_kpi / s_no - KPI_MULTIPLIER).abs() < 1e-9);
    }

    #[test]
    fn floor_drops_trivial_findings() {
        let ctx = ScoringContext::new();
        let trivial = anomaly(0.1);
        let big = anomaly(5.0);
        assert!(!passes_floor(&score(&trivial, &ctx), &ctx));
        assert!(passes_floor(&score(&big, &ctx), &ctx));
    }

    #[test]
    fn floor_requires_significance_not_just_effect() {
        // Large effect but statistically meaningless: z=1.0 (p≈0.32) has
        // effect 0.2 (above min effect) but significance ≈ 0.68... use a
        // clearly insignificant case: uniform concentration over few rows.
        let ctx = ScoringContext::new();
        // HHI barely above uniform for 4 segments (uniform = 0.25) with only
        // 8 rows — chi-square can't reject the null.
        let weak = concentration(0.30, 62.0, 4, 8);
        let b = score(&weak, &ctx);
        assert!(
            b.significance < MIN_SIGNIFICANCE,
            "near-uniform shares on tiny n must not be significant: {}",
            b.significance
        );
        assert!(!passes_floor(&b, &ctx));
    }

    #[test]
    fn z_to_p_approximation() {
        // Known: z=1.96 → p≈0.05
        let p = two_tailed_p_from_z(1.96);
        assert!((p - 0.05).abs() < 0.001, "p={p}");
        // z=2.58 → p≈0.01
        let p = two_tailed_p_from_z(2.58);
        assert!((p - 0.01).abs() < 0.001, "p={p}");
    }

    // ── Null-model behavior ────────────────────────────────────────────────

    #[test]
    fn concentration_uniform_shares_not_significant() {
        // 10 equal segments → HHI = 0.1 = 1/k exactly: X² = 0, p = 1
        let p = p_concentration_vs_uniform(0.1, 10, 1000);
        assert!(p > 0.95, "uniform shares should have p≈1, got {p}");
    }

    #[test]
    fn concentration_dominant_segment_significant() {
        // One segment holds ~90% over 10 segments and plenty of rows
        let hhi = 0.9f64.powi(2) + 9.0 * (0.1f64 / 9.0).powi(2);
        let p = p_concentration_vs_uniform(hhi, 10, 500);
        assert!(p < 0.001, "dominant segment should be significant, got {p}");
    }

    #[test]
    fn concentration_needs_data() {
        // Same HHI but almost no rows → cannot reject
        let p = p_concentration_vs_uniform(0.5, 4, 3);
        assert!((p - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn outlier_cluster_small_share_not_significant() {
        // 2 of 100 columns co-moving across 20 scanned periods is expected noise
        let p = p_outlier_cluster(2, 100, 20);
        assert!(p > 0.5, "2/100 cluster should be unconvincing, got {p}");
    }

    #[test]
    fn outlier_cluster_large_share_significant() {
        // 5 of 6 columns moving together in the same period
        let p = p_outlier_cluster(5, 6, 20);
        assert!(p < 0.01, "5/6 cluster should be significant, got {p}");
    }

    #[test]
    fn membership_small_churn_of_large_base_not_significant() {
        // 5 changed of 500 members — 1% churn, below the 5% baseline
        let p = p_membership_churn(3, 2, 500);
        assert!(p > 0.5, "1% churn should be ordinary, got {p}");
    }

    #[test]
    fn membership_mass_churn_significant() {
        // 30 changed of 60 members — 50% churn
        let p = p_membership_churn(15, 15, 60);
        assert!(p < 0.001, "50% churn should be significant, got {p}");
    }

    #[test]
    fn total_is_product_form() {
        // Zero significance zeroes the total no matter the impact
        let b = ScoreBreakdown {
            significance: 0.0,
            impact: 1.0,
            novelty: 1.0,
            kpi_boost: 2.5,
            polarity_boost: 1.0,
        };
        assert!(total(&b).abs() < f64::EPSILON);
    }

    #[test]
    fn bad_news_is_boosted_and_good_news_is_not() {
        use crate::analysis::tree::TrendDirection;
        let trend = |direction: TrendDirection| AnalysisType::Trend {
            column: "churn".to_string(),
            direction,
            slope: 1.0,
            r_squared: 0.9,
            p_value: 0.001,
        };
        let mut polarity = HashMap::new();
        polarity.insert("churn".to_string(), Polarity::LowerIsBetter);
        let ctx = ScoringContext::new().with_polarity(polarity);
        assert!(
            (polarity_boost_for(&trend(TrendDirection::Increasing), &ctx) - BAD_NEWS_MULTIPLIER)
                .abs()
                < 1e-12
        );
        assert!((polarity_boost_for(&trend(TrendDirection::Decreasing), &ctx) - 1.0).abs() < 1e-12);
        // No declared polarity, or a finding without a direction: untouched.
        assert!(
            (polarity_boost_for(&trend(TrendDirection::Increasing), &ScoringContext::new()) - 1.0)
                .abs()
                < 1e-12
        );
        let undirected = AnalysisType::Concentration {
            column: "churn".to_string(),
            segment_column: "region".to_string(),
            hhi: 0.8,
            top_n: 1,
            top_share: 0.9,
            hhi_delta: None,
            n_segments: 3,
            n_rows: 100,
        };
        assert!((polarity_boost_for(&undirected, &ctx) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_declared_metric_names_a_derived_series_as_kpi() {
        use brightflow_types::{Aggregation as A, FilterOp, MetricExpr, MetricExt, MetricFilter};
        let ext = MetricExt {
            expr: MetricExpr {
                dataset: None,
                column: "reactions_total".to_string(),
                aggregation: A::Sum,
                filters: vec![MetricFilter {
                    column: "state".to_string(),
                    op: FilterOp::Eq,
                    value: Some(serde_json::Value::String("open".to_string())),
                }],
            },
            is_kpi: Some(true),
            polarity: None,
            format: None,
        };
        let metric = MetricSpec::from_ext("open_reactions", &ext);
        let ctx = ScoringContext::new().with_metrics(vec![metric]);
        let measure = MeasureRef::Column("reactions_total".to_string());
        let open = vec![FilterSpec {
            column: "state".to_string(),
            value: "open".to_string(),
        }];
        assert!(
            (ctx.series_kpi_boost(&measure, Aggregation::Sum, &open) - KPI_MULTIPLIER).abs()
                < 1e-12
        );
        // A deeper slice under the metric's filter still counts; the wrong
        // aggregation, the unfiltered column, or another column do not.
        let deeper = vec![
            open[0].clone(),
            FilterSpec {
                column: "region".to_string(),
                value: "eu".to_string(),
            },
        ];
        assert!(
            (ctx.series_kpi_boost(&measure, Aggregation::Sum, &deeper) - KPI_MULTIPLIER).abs()
                < 1e-12
        );
        assert!((ctx.series_kpi_boost(&measure, Aggregation::Mean, &open) - 1.0).abs() < 1e-12);
        assert!((ctx.series_kpi_boost(&measure, Aggregation::Sum, &[]) - 1.0).abs() < 1e-12);
        assert!(
            (ctx.series_kpi_boost(
                &MeasureRef::Column("comments".to_string()),
                Aggregation::Sum,
                &open
            ) - 1.0)
                .abs()
                < 1e-12
        );
        // An aggregation the enumeration never produces can never match.
        let median = MetricSpec::from_ext(
            "m",
            &MetricExt {
                expr: MetricExpr {
                    aggregation: A::Median,
                    ..ext.expr.clone()
                },
                ..ext.clone()
            },
        );
        assert_eq!(median.aggregation, None);
    }

    #[test]
    fn novelty_halves_stale_stories() {
        let fresh = ScoreBreakdown {
            significance: 1.0,
            impact: 1.0,
            novelty: 1.0,
            kpi_boost: 1.0,
            polarity_boost: 1.0,
        };
        let stale = ScoreBreakdown {
            novelty: 0.0,
            ..fresh.clone()
        };
        assert!((total(&stale) / total(&fresh) - 0.5).abs() < 1e-9);
    }
}

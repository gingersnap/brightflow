//! Calibrated interestingness scoring.
//!
//! Replaces the ad-hoc `change_percent / p_value` formulas previously scattered
//! across detectors. Combines three normalized components:
//! - **Significance**: 1 - p_value, clamped
//! - **Effect size**: per-type, normalized to [0, 1] using effect-size conventions
//!   (Cohen's d, |r|, R², z/5, etc.)
//! - **Surprise**: how unusual this finding is relative to the dataset's own baseline
//!   (computed within-run; no historical store yet)
//!
//! The final score is a weighted product:
//!   score = (W_SIG * sig + W_EFFECT * effect + W_SURPRISE * surprise) * kpi_boost
//!
//! Calibration is iterative — tune the constants empirically on real datasets.

// Match arms over `AnalysisType` are intentionally exhaustive per variant for
// readability and so that adding a new variant raises a compile error rather
// than silently falling into a default. Suppress the related nursery lints.
#![allow(clippy::match_same_arms)]

use std::collections::HashSet;

use crate::analysis::tree::{AnalysisType, ScoreBreakdown, TrendDirection};

// ─── Calibration constants ────────────────────────────────────────────────────

/// Weight for significance (1 - p_value)
const W_SIG: f64 = 0.30;
/// Weight for effect size (normalized magnitude)
const W_EFFECT: f64 = 0.45;
/// Weight for surprise (within-run novelty)
const W_SURPRISE: f64 = 0.25;

/// Multiplier applied when a finding's measure is flagged as KPI
const KPI_MULTIPLIER: f64 = 2.5;

/// Effect-size floor — findings below this are dropped entirely
const DEFAULT_MIN_EFFECT: f64 = 0.05;

// ─── Scoring context ──────────────────────────────────────────────────────────

/// Context passed to scoring — carries dataset-wide info needed for surprise
/// and KPI flagging. Fields can be empty (no-ops) for the basic case.
#[derive(Debug, Clone, Default)]
pub struct ScoringContext {
    /// Columns flagged as KPIs (semantic layer hook — empty for now)
    pub kpi_columns: HashSet<String>,
    /// Min effect size (override DEFAULT_MIN_EFFECT)
    pub min_effect_size: Option<f64>,
}

impl ScoringContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_kpis(mut self, kpis: HashSet<String>) -> Self {
        self.kpi_columns = kpis;
        self
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
    let effect = effect_size_component(analysis);
    // Surprise is within-run; without history, we use the effect size as a
    // proxy weighted by significance. Replaceable later with JS divergence
    // vs run-baseline once we accumulate per-detector populations.
    let surprise = (sig * effect).sqrt();
    let kpi_boost = kpi_boost_for(analysis, ctx);

    ScoreBreakdown {
        significance: sig,
        effect_size: effect,
        surprise,
        kpi_boost,
    }
}

/// Combine the breakdown into a single scalar.
pub fn total(b: &ScoreBreakdown) -> f64 {
    let weighted = W_SURPRISE.mul_add(
        b.surprise,
        W_EFFECT.mul_add(b.effect_size, W_SIG * b.significance),
    );
    weighted * b.kpi_boost
}

/// Returns true if a finding should be kept (effect size above the floor)
pub fn passes_floor(b: &ScoreBreakdown, ctx: &ScoringContext) -> bool {
    b.effect_size >= ctx.min_effect()
}

// ─── Component computations ───────────────────────────────────────────────────

fn significance_component(a: &AnalysisType) -> f64 {
    let p = match a {
        AnalysisType::Anomaly { z_score, .. } => {
            // No explicit p-value on anomalies — derive from |z| via normal CDF approx
            // p ≈ 2 * (1 - Φ(|z|)) ; quick approximation below saves a call
            two_tailed_p_from_z(*z_score)
        },
        AnalysisType::Segment { p_value, .. } => *p_value,
        AnalysisType::Correlation { p_value, .. } => *p_value,
        AnalysisType::Trend { p_value, .. } => *p_value,
        AnalysisType::PeriodComparison { p_value, .. } => *p_value,
        AnalysisType::PeriodAnomaly { p_value, .. } => *p_value,
        AnalysisType::Seasonality { p_value, .. } => *p_value,
        AnalysisType::ForecastDeviation { p_value, .. } => *p_value,
        AnalysisType::OutlierCluster { cluster_size, .. } => {
            // Bigger clusters less likely under H0 — synthetic p
            (-(*cluster_size as f64) * 0.5).exp()
        },
        AnalysisType::Concentration { hhi, .. } => {
            // Higher HHI is more "significant" — synthetic
            (1.0 - hhi).clamp(0.0, 1.0)
        },
        AnalysisType::DistributionShift { p_value, .. } => *p_value,
        AnalysisType::MembershipChange {
            added_count,
            removed_count,
            ..
        } => {
            // More changes = more significant — synthetic
            (-((*added_count + *removed_count) as f64) * 0.3).exp()
        },
        AnalysisType::ChangePoint { p_value, .. } => *p_value,
    };
    (1.0 - p.clamp(0.0, 1.0)).clamp(0.0, 1.0)
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
        AnalysisType::OutlierCluster { cluster_size, .. } => {
            clamp_unit((*cluster_size as f64) / 5.0)
        },
        AnalysisType::Concentration { hhi, top_share, .. } => {
            // Concentration is interesting when HHI is high OR top_share is dominant
            clamp_unit(hhi.max(*top_share / 100.0))
        },
        AnalysisType::DistributionShift { ks_statistic, .. } => clamp_unit(*ks_statistic),
        AnalysisType::MembershipChange {
            added_count,
            removed_count,
            ..
        } => clamp_unit(((*added_count + *removed_count) as f64) / 10.0),
        AnalysisType::ChangePoint {
            before_mean,
            after_mean,
            ..
        } => {
            // Effect = relative change at the step
            let denom = before_mean.abs().max(1e-6);
            clamp_unit((after_mean - before_mean).abs() / denom)
        },
    }
}

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
#[allow(clippy::shadow_unrelated)]
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
        assert!(s_kpi > s_no * 2.0);
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
    fn z_to_p_approximation() {
        // Known: z=1.96 → p≈0.05
        let p = two_tailed_p_from_z(1.96);
        assert!((p - 0.05).abs() < 0.001, "p={p}");
        // z=2.58 → p≈0.01
        let p = two_tailed_p_from_z(2.58);
        assert!((p - 0.01).abs() < 0.001, "p={p}");
    }
}

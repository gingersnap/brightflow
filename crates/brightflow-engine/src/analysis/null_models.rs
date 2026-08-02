//! Type-specific "boring baseline" null models.
//!
//! Every detector's finding is scored against an explicit null hypothesis of
//! what an *uninteresting* dataset would look like (QuickInsights/Top-K
//! Insights): a trend against slope ≈ 0, a change point against a stable
//! series, a dominant leader against the power-law share expected from its
//! rank distribution. This makes significance comparable across detector
//! types — the property the old per-detector ad-hoc formulas never had.
//!
//! **Limitation — calibration is synthetic-only.** The false-positive and power
//! tests in this module's test block run against deterministic pseudo-noise, not
//! against a corpus of real business tables. That keeps the harness reproducible
//! and dependency-free, but real data has autocorrelation and heavy tails the
//! generators don't model. Treat the ~5% false-positive targets as design intent,
//! not a measured guarantee. Lifting this needs a labelled corpus of real tables
//! to calibrate against.

use statrs::distribution::{ContinuousCDF, Normal, StudentsT};

use crate::stats::significance::{
    linear_regression, mean, p_value_for_correlation, p_value_welch_t_test, std_dev,
};

/// Series shorter than this get their score capped and flagged low-confidence:
/// p-values from a handful of points are numerically valid but epistemically
/// weak.
pub const MIN_CONFIDENT_POINTS: usize = 6;
/// Score cap applied below `MIN_CONFIDENT_POINTS`.
const LOW_CONFIDENCE_CAP: f64 = 0.6;

/// Outcome of testing a finding against its null model.
#[derive(Debug, Clone, Copy)]
pub struct Significance {
    /// Probability of the observation under the boring baseline.
    pub p_value: f64,
    /// Normalized significance in [0, 1] — comparable across detector types.
    pub score: f64,
    /// True when the series was too short for a confident verdict.
    pub low_confidence: bool,
}

impl Significance {
    fn new(p_value: f64, raw_score: f64, n: usize) -> Self {
        let low_confidence = n < MIN_CONFIDENT_POINTS;
        let mut score = raw_score.clamp(0.0, 1.0);
        if low_confidence {
            score = score.min(LOW_CONFIDENCE_CAP);
        }
        Self {
            p_value: p_value.clamp(0.0, 1.0),
            score,
            low_confidence,
        }
    }

    pub fn none() -> Self {
        Self {
            p_value: 1.0,
            score: 0.0,
            low_confidence: true,
        }
    }
}

/// Trend null: slope ≈ 0. `score = r² · (1 − p)` — a steep slope through a
/// noise cloud is not a story; a clean line is.
pub fn trend_null(values: &[f64]) -> Significance {
    let n = values.len();
    if n < 3 {
        return Significance::none();
    }
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let Some((_slope, _intercept, r_squared)) = linear_regression(&x, values) else {
        return Significance::none();
    };
    let r = r_squared.max(0.0).sqrt();
    let p = p_value_for_correlation(r, n);
    Significance::new(p, r_squared * (1.0 - p), n)
}

/// Change-point null: a stable series. Welch t-test of before vs after means,
/// Bonferroni-corrected for having scanned every interior position.
pub fn change_point_null(values: &[f64], cp_idx: usize) -> Significance {
    let n = values.len();
    if n < 6 || cp_idx < 2 || cp_idx + 3 > n {
        return Significance::none();
    }
    let before = &values[..=cp_idx];
    let after = &values[cp_idx + 1..];
    let p_single = p_value_welch_t_test(
        mean(before),
        std_dev(before),
        before.len(),
        mean(after),
        std_dev(after),
        after.len(),
    );
    // We picked the best of (n - 4) candidate positions.
    let positions = (n.saturating_sub(4)).max(1) as f64;
    let p = (p_single * positions).clamp(0.0, 1.0);
    Significance::new(p, 1.0 - p, n)
}

/// Point null: last value against a *detrended* Gaussian of the history.
/// Detrending stops steady growth from flagging every new period as an
/// anomaly.
pub fn point_null(values: &[f64]) -> Significance {
    let n = values.len();
    if n < 4 {
        return Significance::none();
    }
    let history = &values[..n - 1];
    let hist_n = history.len();
    let x: Vec<f64> = (0..hist_n).map(|i| i as f64).collect();
    let (residuals, expected_last): (Vec<f64>, f64) = match linear_regression(&x, history) {
        Some((slope, intercept, _)) => {
            let resid = history
                .iter()
                .enumerate()
                .map(|(i, v)| v - slope.mul_add(i as f64, intercept))
                .collect();
            (resid, slope.mul_add(hist_n as f64, intercept))
        },
        None => (history.to_vec(), mean(history)),
    };
    let sigma = std_dev(&residuals);
    if sigma <= 0.0 || !sigma.is_finite() {
        return Significance::none();
    }
    let last = values[n - 1];
    let z = (last - expected_last) / sigma;
    let p = two_tailed_p(z);
    Significance::new(p, 1.0 - p, n)
}

/// Top-1 dominance null (Top-K Insights): fit a power law to the shares of
/// ranks 2..k in log-log space, predict what rank 1 "should" hold, and test
/// the leader's excess against the fit's residual spread.
///
/// `shares` must be sorted descending and sum to ≈ 1.
pub fn top1_null(shares: &[f64]) -> Significance {
    let k = shares.len();
    if k < 4 {
        return Significance::none();
    }
    // log(share) = a − b·log(rank), fitted on ranks 2..k
    let xs: Vec<f64> = (2..=k).map(|r| (r as f64).ln()).collect();
    let ys: Vec<f64> = shares[1..].iter().map(|s| s.max(1e-12).ln()).collect();
    let Some((slope, intercept, _)) = linear_regression(&xs, &ys) else {
        return Significance::none();
    };
    let residuals: Vec<f64> = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| y - slope.mul_add(*x, intercept))
        .collect();
    let sigma = std_dev(&residuals).max(1e-6);
    let predicted_log_top = intercept; // ln(rank 1) = 0
    let observed_log_top = shares[0].max(1e-12).ln();
    let z = (observed_log_top - predicted_log_top) / sigma;
    // One-sided: only an unexpectedly LARGE leader is a dominance story.
    let p = one_tailed_upper_p(z);
    Significance::new(p, 1.0 - p, k)
}

/// Rank-change null: uniform rank shuffling.
///
/// How unlikely is a jump of `|rank_delta|` positions among `n_siblings` if
/// ranks re-draw uniformly? We use the simple form for a fixed starting rank
/// — P = (n − d)/n per endpoint, squared for observing both ends.
pub fn rank_change_null(rank_delta: usize, n_siblings: usize, n_periods: usize) -> Significance {
    if n_siblings < 3 || rank_delta == 0 {
        return Significance::none();
    }
    let n = n_siblings as f64;
    let d = rank_delta as f64;
    // P(new rank at least d away from old) under uniform re-draw
    let p_single = ((n - d) / n).clamp(0.0, 1.0);
    // The bigger the jump relative to the field, the smaller p.
    let p = p_single.powi(2); // squared: both endpoints observed
    Significance::new(p, 1.0 - p, n_periods)
}

/// Correlation null: r = 0 (t-distributed).
pub fn correlation_null(r: f64, n: usize) -> Significance {
    if n < 4 {
        return Significance::none();
    }
    let p = p_value_for_correlation(r, n);
    Significance::new(p, r.abs() * (1.0 - p), n)
}

/// Two-sample comparison null (current vs previous period).
pub fn comparison_null(
    curr_mean: f64,
    curr_std: f64,
    curr_n: usize,
    prev_mean: f64,
    prev_std: f64,
    prev_n: usize,
) -> Significance {
    let p = p_value_welch_t_test(curr_mean, curr_std, curr_n, prev_mean, prev_std, prev_n);
    Significance::new(p, 1.0 - p, curr_n.min(prev_n))
}

fn two_tailed_p(z: f64) -> f64 {
    match Normal::new(0.0, 1.0) {
        Ok(normal) => (2.0 * (1.0 - normal.cdf(z.abs()))).clamp(0.0, 1.0),
        Err(_) => 1.0,
    }
}

fn one_tailed_upper_p(z: f64) -> f64 {
    match Normal::new(0.0, 1.0) {
        Ok(normal) => (1.0 - normal.cdf(z)).clamp(0.0, 1.0),
        Err(_) => 1.0,
    }
}

/// Critical t helper exposed for tests.
#[allow(dead_code)]
fn t_cdf(t: f64, df: f64) -> f64 {
    StudentsT::new(0.0, 1.0, df).map_or(0.5, |d| d.cdf(t))
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::suboptimal_flops
)]
mod tests {
    use super::*;

    /// Deterministic pseudo-noise in [-0.5, 0.5).
    fn noise(i: usize, salt: u64) -> f64 {
        let mut z = (i as u64)
            .wrapping_add(salt)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z ^= z >> 27;
        ((z % 10_000) as f64 / 10_000.0) - 0.5
    }

    // ── Calibration: false-positive rate under H0 ─────────────────────────

    #[test]
    fn trend_null_fp_rate_on_flat_series() {
        let mut fp = 0;
        let runs = 400;
        for salt in 0..runs {
            let values: Vec<f64> = (0..24).map(|i| 100.0 + noise(i, salt) * 8.0).collect();
            let sig = trend_null(&values);
            if sig.p_value < 0.05 {
                fp += 1;
            }
        }
        let rate = fp as f64 / runs as f64;
        assert!(
            rate < 0.10,
            "flat-series trend FP rate should be ≈5%, got {rate:.3}"
        );
    }

    #[test]
    fn trend_null_power_on_planted_trend() {
        let mut hits = 0;
        let runs = 200;
        for salt in 0..runs {
            let values: Vec<f64> = (0..24)
                .map(|i| 100.0 + (i as f64) * 2.0 + noise(i, salt) * 8.0)
                .collect();
            let sig = trend_null(&values);
            if sig.p_value < 0.05 && sig.score > 0.5 {
                hits += 1;
            }
        }
        let power = hits as f64 / runs as f64;
        assert!(
            power > 0.8,
            "planted trend power should be >80%, got {power:.3}"
        );
    }

    #[test]
    fn change_point_null_fp_rate_on_stable_series() {
        let mut fp = 0;
        let runs = 300;
        for salt in 0..runs {
            let values: Vec<f64> = (0..20).map(|i| 50.0 + noise(i, salt) * 6.0).collect();
            // Test the "best-looking" position like a scanner would
            let best = (2..17)
                .map(|cp| (cp, change_point_null(&values, cp)))
                .min_by(|a, b| a.1.p_value.partial_cmp(&b.1.p_value).unwrap())
                .unwrap();
            if best.1.p_value < 0.05 {
                fp += 1;
            }
        }
        let rate = fp as f64 / runs as f64;
        assert!(
            rate < 0.12,
            "stable-series change-point FP rate too high: {rate:.3}"
        );
    }

    #[test]
    fn change_point_null_power_on_planted_step() {
        let mut hits = 0;
        let runs = 200;
        for salt in 0..runs {
            let values: Vec<f64> = (0..20)
                .map(|i| if i < 10 { 50.0 } else { 80.0 } + noise(i, salt) * 6.0)
                .collect();
            let sig = change_point_null(&values, 9);
            if sig.p_value < 0.05 {
                hits += 1;
            }
        }
        let power = hits as f64 / runs as f64;
        assert!(
            power > 0.8,
            "planted step power should be >80%, got {power:.3}"
        );
    }

    #[test]
    fn point_null_detrended_growth_is_boring() {
        // Steady growth: the newest point continues the line → NOT anomalous
        let values: Vec<f64> = (0..20).map(|i| 100.0 + (i as f64) * 5.0).collect();
        let sig = point_null(&values);
        assert!(
            sig.p_value > 0.2,
            "on-trend point must not be anomalous, p={}",
            sig.p_value
        );
    }

    #[test]
    fn point_null_flags_genuine_spike() {
        let mut values: Vec<f64> = (0..20).map(|i| 100.0 + noise(i, 3) * 4.0).collect();
        values.push(400.0);
        let sig = point_null(&values);
        assert!(
            sig.p_value < 0.001,
            "spike must be flagged, p={}",
            sig.p_value
        );
        assert!(!sig.low_confidence);
    }

    #[test]
    fn top1_null_powerlaw_leader_is_boring() {
        // Zipf-ish shares: leader exactly where the power law predicts
        let raw: Vec<f64> = (1..=10).map(|r| 1.0 / f64::from(r)).collect();
        let total: f64 = raw.iter().sum();
        let shares: Vec<f64> = raw.iter().map(|v| v / total).collect();
        let sig = top1_null(&shares);
        assert!(
            sig.p_value > 0.2,
            "zipf leader is expected, p={}",
            sig.p_value
        );
    }

    #[test]
    fn top1_null_flags_excess_dominance() {
        // Leader holds 85%, the rest follow a shallow tail
        let mut shares = vec![0.85];
        let tail_total = 0.15;
        let raw_tail: Vec<f64> = (2..=10).map(|r| 1.0 / f64::from(r)).collect();
        let tail_sum: f64 = raw_tail.iter().sum();
        shares.extend(raw_tail.iter().map(|v| v / tail_sum * tail_total));
        let sig = top1_null(&shares);
        assert!(
            sig.p_value < 0.05,
            "excess dominance must be flagged, p={}",
            sig.p_value
        );
    }

    #[test]
    fn short_series_capped_as_low_confidence() {
        let values = vec![1.0, 2.0, 3.0, 30.0];
        let sig = point_null(&values);
        assert!(sig.low_confidence);
        assert!(sig.score <= 0.6);
    }

    #[test]
    fn rank_change_big_jump_in_big_field_significant() {
        let big = rank_change_null(8, 10, 12);
        let small = rank_change_null(1, 10, 12);
        assert!(big.score > small.score);
        assert!(
            big.p_value < 0.05,
            "8-place jump in a field of 10: p={}",
            big.p_value
        );
    }
}

//! Direction and strength of change over time, by linear regression.
//!
//! Reports slope, r-squared, and p-value together because a steep slope through
//! scattered points is not a trend — all three are needed before calling a
//! direction.

use anyhow::Result;
use polars::prelude::*;

use crate::analysis::tree::TrendDirection;
use crate::stats::significance::{linear_regression, p_value_for_correlation};

#[derive(Debug, Clone)]
pub struct TrendResult {
    pub column: String,
    pub direction: TrendDirection,
    pub slope: f64,
    pub r_squared: f64,
    pub p_value: f64,
}

/// Detect trend in an ordered series of values (typically period-aggregated
/// means, one point per period — regressing on raw row index conflates
/// row order with time and inflates n).
pub fn detect_trend_in_series(column: &str, values: &[f64]) -> Option<TrendResult> {
    if values.len() < 3 {
        return None;
    }

    let x: Vec<f64> = (0..values.len()).map(|i| i as f64).collect();

    let (slope, _intercept, r_squared) = linear_regression(&x, values)?;

    let r = r_squared.sqrt();
    let p_value = p_value_for_correlation(r, values.len());

    let direction = if slope > 0.0 {
        TrendDirection::Increasing
    } else {
        TrendDirection::Decreasing
    };

    Some(TrendResult {
        column: column.to_string(),
        direction,
        slope,
        r_squared,
        p_value,
    })
}

/// Detect trend directly over a numeric column's raw rows.
///
/// Fallback for datasets without a time column — when periods exist, callers
/// should aggregate first and use `detect_trend_in_series`.
pub fn detect_trend(df: &DataFrame, column: &str) -> Result<Option<TrendResult>> {
    let col = df.column(column)?;

    let values: Vec<f64> = col
        .cast(&DataType::Float64)?
        .f64()?
        .into_iter()
        .flatten()
        .collect();

    Ok(detect_trend_in_series(column, &values))
}

#[cfg(test)]
#[expect(
    clippy::suboptimal_flops,
    reason = "test math is written for readability, not FLOP count"
)]
mod tests {
    use super::*;

    #[test]
    fn detects_clean_upward_trend() {
        let values: Vec<f64> = (0..12).map(|i| 100.0 + f64::from(i) * 5.0).collect();
        let r = detect_trend_in_series("x", &values).unwrap();
        assert!(matches!(r.direction, TrendDirection::Increasing));
        assert!(r.r_squared > 0.99);
        assert!(r.p_value < 0.001);
    }

    #[test]
    fn flat_series_has_no_significant_trend() {
        let values = vec![100.0, 101.0, 99.0, 100.5, 99.5, 100.0, 100.2, 99.8];
        let r = detect_trend_in_series("x", &values).unwrap();
        assert!(r.r_squared < 0.3, "r²={}", r.r_squared);
    }
}

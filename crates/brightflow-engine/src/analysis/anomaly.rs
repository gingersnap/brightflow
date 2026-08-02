use anyhow::Result;
use polars::prelude::*;

use crate::stats::significance::{mean, std_dev, z_score};

#[derive(Debug, Clone)]
pub struct AnomalyResult {
    pub column: String,
    pub value: f64,
    pub mean: f64,
    pub std_dev: f64,
    pub z_score: f64,
}

/// Test the last value of an ordered series against the rest as history.
///
/// Pass period-aggregated values (one point per period) so the test asks
/// "is the latest *period* unusual?" — testing the last raw row against all
/// rows answers a question nobody asked.
pub fn detect_anomaly_in_series(column: &str, values: &[f64]) -> Option<AnomalyResult> {
    if values.len() < 4 {
        return None;
    }

    let (&latest_value, historical) = values.split_last()?;

    let col_mean = mean(historical);
    let col_std = std_dev(historical);

    if col_std == 0.0 {
        return None;
    }

    let z = z_score(latest_value, col_mean, col_std);

    Some(AnomalyResult {
        column: column.to_string(),
        value: latest_value,
        mean: col_mean,
        std_dev: col_std,
        z_score: z,
    })
}

/// Detect anomaly in the latest raw value of a numeric column.
///
/// Fallback for datasets without a time column — when periods exist, callers
/// should aggregate first and use `detect_anomaly_in_series`.
pub fn detect_anomaly(df: &DataFrame, column: &str) -> Result<Option<AnomalyResult>> {
    let col = df.column(column)?;

    let values: Vec<f64> = col
        .cast(&DataType::Float64)?
        .f64()?
        .into_iter()
        .flatten()
        .collect();

    Ok(detect_anomaly_in_series(column, &values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_spike_in_last_period() {
        let values = vec![100.0, 102.0, 98.0, 101.0, 99.0, 400.0];
        let r = detect_anomaly_in_series("x", &values).unwrap();
        assert!(r.z_score > 3.0, "z={}", r.z_score);
    }

    #[test]
    fn ordinary_last_period_is_not_anomalous() {
        let values = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.5];
        let r = detect_anomaly_in_series("x", &values).unwrap();
        assert!(r.z_score.abs() < 1.0, "z={}", r.z_score);
    }

    #[test]
    fn too_short_series_returns_none() {
        assert!(detect_anomaly_in_series("x", &[1.0, 2.0, 3.0]).is_none());
    }
}

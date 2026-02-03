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

/// Detect trend in a numeric column
pub fn detect_trend(df: &DataFrame, column: &str) -> Result<Option<TrendResult>> {
    let col = df.column(column)?;

    let values: Vec<f64> = col
        .cast(&DataType::Float64)?
        .f64()?
        .into_iter()
        .flatten()
        .collect();

    if values.len() < 3 {
        return Ok(None);
    }

    let x: Vec<f64> = (0..values.len()).map(|i| i as f64).collect();

    let (slope, _intercept, r_squared) = match linear_regression(&x, &values) {
        Some(result) => result,
        None => return Ok(None),
    };

    let r = r_squared.sqrt();
    let p_value = p_value_for_correlation(r, values.len());

    let direction = if slope > 0.0 {
        TrendDirection::Increasing
    } else {
        TrendDirection::Decreasing
    };

    Ok(Some(TrendResult {
        column: column.to_string(),
        direction,
        slope,
        r_squared,
        p_value,
    }))
}

use anyhow::Result;
use polars::prelude::*;

use crate::stats::significance::{p_value_for_correlation, pearson_correlation};

#[derive(Debug, Clone)]
pub struct CorrelationResult {
    pub column_a: String,
    pub column_b: String,
    pub r_value: f64,
    pub p_value: f64,
}

/// Calculate correlation between two numeric columns
pub fn correlate(
    df: &DataFrame,
    col_a: &str,
    col_b: &str,
) -> Result<Option<CorrelationResult>> {
    let a = df.column(col_a)?;
    let b = df.column(col_b)?;

    let a_values: Vec<f64> = a
        .cast(&DataType::Float64)?
        .f64()?
        .into_iter()
        .flatten()
        .collect();

    let b_values: Vec<f64> = b
        .cast(&DataType::Float64)?
        .f64()?
        .into_iter()
        .flatten()
        .collect();

    let pairs: Vec<(f64, f64)> = a_values
        .into_iter()
        .zip(b_values.into_iter())
        .collect();

    if pairs.len() < 3 {
        return Ok(None);
    }

    let x: Vec<f64> = pairs.iter().map(|(a, _)| *a).collect();
    let y: Vec<f64> = pairs.iter().map(|(_, b)| *b).collect();

    let r = match pearson_correlation(&x, &y) {
        Some(r) => r,
        None => return Ok(None),
    };

    let p_value = p_value_for_correlation(r, pairs.len());

    Ok(Some(CorrelationResult {
        column_a: col_a.to_string(),
        column_b: col_b.to_string(),
        r_value: r,
        p_value,
    }))
}

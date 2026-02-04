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

/// Detect anomaly in the latest value of a numeric column
pub fn detect_anomaly(df: &DataFrame, column: &str) -> Result<Option<AnomalyResult>> {
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

    // Safe: we checked len >= 3 above
    let Some(&latest_value) = values.last() else {
        return Ok(None);
    };
    let Some(historical) = values.get(..values.len() - 1) else {
        return Ok(None);
    };

    let col_mean = mean(historical);
    let col_std = std_dev(historical);

    if col_std == 0.0 {
        return Ok(None);
    }

    let z = z_score(latest_value, col_mean, col_std);

    Ok(Some(AnomalyResult {
        column: column.to_string(),
        value: latest_value,
        mean: col_mean,
        std_dev: col_std,
        z_score: z,
    }))
}

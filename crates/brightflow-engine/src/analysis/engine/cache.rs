//! Pre-extracted column data shared by the analysis passes.

use std::collections::HashMap;

use anyhow::Result;
use polars::prelude::*;

use crate::data::schema::DataSchema;

/// Pre-extracted column data to avoid repeated DataFrame access
pub struct ColumnCache {
    pub numeric: HashMap<String, Vec<f64>>,
    pub dimension: HashMap<String, Vec<String>>,
}

impl ColumnCache {
    pub fn new(df: &DataFrame, schema: &DataSchema) -> Result<Self> {
        let mut numeric = HashMap::new();
        let mut dimension = HashMap::new();

        // Extract all numeric columns (KPIs + metrics)
        for col in &schema.measure_columns {
            if let Ok(series) = df.column(col) {
                let values: Vec<f64> = series
                    .cast(&DataType::Float64)?
                    .f64()?
                    .into_iter()
                    .flatten()
                    .collect();
                numeric.insert(col.clone(), values);
            }
        }

        // Extract all dimension columns
        for col in &schema.dimension_columns {
            if let Ok(series) = df.column(col) {
                let values: Vec<String> = series
                    .cast(&DataType::String)?
                    .str()?
                    .into_iter()
                    .map(|v| v.unwrap_or("").to_string())
                    .collect();
                dimension.insert(col.clone(), values);
            }
        }

        Ok(Self { numeric, dimension })
    }

    /// Check if a dimension has meaningful data in a specific period
    /// Returns (rows_in_period, unique_values_in_period)
    pub fn dimension_coverage_in_period(
        &self,
        segment_col: &str,
        period: &str,
        period_labels: &[Option<String>],
    ) -> (usize, usize) {
        let Some(segment_values) = self.dimension.get(segment_col) else {
            return (0, 0);
        };

        let mut values_in_period: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        let mut count = 0;

        for (seg_val, period_label) in segment_values.iter().zip(period_labels.iter()) {
            if let Some(p) = period_label {
                if p == period {
                    count += 1;
                    if !seg_val.is_empty() {
                        values_in_period.insert(seg_val);
                    }
                }
            }
        }

        (count, values_in_period.len())
    }
}

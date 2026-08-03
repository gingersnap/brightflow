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
}

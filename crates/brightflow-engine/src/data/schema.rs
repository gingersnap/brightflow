//! The resolved analysis schema: which columns are measures, dimensions, time.
//!
//! A schema is what the data actually supports, resolved from user-declared
//! semantics (see `data::merge`) or auto-detected from dtypes. Every generator
//! downstream can assume the columns it is handed exist and have the type it
//! expects.

use std::collections::HashMap;

use anyhow::Result;
use polars::prelude::*;

use crate::data::config::{Polarity, TimeGranularity};

#[derive(Debug, Clone)]
pub struct DataSchema {
    /// All numeric measure columns (both KPIs and supporting metrics)
    pub measure_columns: Vec<String>,
    /// Subset of measure_columns that are KPIs (highest priority)
    pub kpi_columns: Vec<String>,
    /// Categorical columns for segmentation
    pub dimension_columns: Vec<String>,
    /// Time columns for temporal analysis
    pub time_columns: Vec<String>,
    /// Primary time column for trends
    pub time_column: Option<String>,
    /// Time granularity for period comparisons
    pub time_granularity: TimeGranularity,
    /// Per-measure polarity (only non-neutral entries; absent = neutral)
    pub polarity: HashMap<String, Polarity>,
}

impl DataSchema {
    /// All numeric columns that should be analyzed
    pub fn analyzable_columns(&self) -> Vec<String> {
        self.measure_columns.clone()
    }

    /// Check if a column is a KPI (higher priority)
    pub fn is_kpi(&self, column: &str) -> bool {
        self.kpi_columns.contains(&column.to_string())
    }
}

/// Auto-detect schema from DataFrame (fallback when no config provided)
pub fn detect_schema(df: &DataFrame) -> Result<DataSchema> {
    let mut measure_columns = Vec::new();
    let mut dimension_columns = Vec::new();
    let mut time_columns = Vec::new();
    let mut time_column = None;

    let row_count = df.height();

    for col in df.get_columns() {
        let name = col.name().to_string();
        let dtype = col.dtype();

        match dtype {
            DataType::Date | DataType::Datetime(_, _) => {
                time_columns.push(name.clone());
                if time_column.is_none() {
                    time_column = Some(name);
                }
            },
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float32
            | DataType::Float64 => {
                let unique_count = col.n_unique().unwrap_or(0);
                let cardinality_ratio = unique_count as f64 / row_count as f64;

                if unique_count < 20 && cardinality_ratio < 0.05 {
                    dimension_columns.push(name);
                } else {
                    measure_columns.push(name);
                }
            },
            DataType::String => {
                dimension_columns.push(name.clone());

                let name_lower = name.to_lowercase();
                if time_column.is_none()
                    && (name_lower == "date" || name_lower == "timestamp" || name_lower == "time")
                {
                    time_column = Some(name);
                }
            },
            _ => {
                if dtype.is_categorical() {
                    dimension_columns.push(name);
                }
            },
        }
    }

    if time_column.is_none() && !df.get_columns().is_empty() {
        let first_col = df.get_columns()[0].name().to_string();
        let first_lower = first_col.to_lowercase();
        if first_lower.contains("date") || first_lower.contains("time") {
            time_column = Some(first_col);
        }
    }

    Ok(DataSchema {
        measure_columns,
        kpi_columns: Vec::new(), // Auto-detect doesn't distinguish KPIs
        dimension_columns,
        time_columns,
        time_column,
        time_granularity: TimeGranularity::default(),
        polarity: HashMap::new(),
    })
}

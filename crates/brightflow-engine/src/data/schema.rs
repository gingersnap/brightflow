//! The resolved analysis schema: which columns are measures, dimensions, time.
//!
//! A schema is what the data actually supports, resolved from user-declared
//! semantics (see `data::merge`) or auto-detected from dtypes. Every generator
//! downstream can assume the columns it is handed exist and have the type it
//! expects.

use std::collections::HashMap;

use anyhow::Result;
use polars::prelude::*;

use crate::data::config::{ColumnRole, Polarity, TimeGranularity};
use brightflow_types::{ColumnExt, Dataset, Field, LogicalType, MetricExt, TableDeclaration};

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
    /// Identifier columns: not analysed, but named so a finding can say
    /// "per user" rather than treating the column as noise.
    pub entity_columns: Vec<String>,
    /// Display labels someone set; `label_for` falls back to the name.
    pub labels: HashMap<String, String>,
    /// Descriptions someone set, for narration and for the LLM.
    pub descriptions: HashMap<String, String>,
    /// Metrics someone declared over this table's columns.
    pub metrics: Vec<DeclaredMetric>,
}

/// A named metric from the resolved semantics: the contract's structured
/// form plus its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredMetric {
    pub name: String,
    pub ext: MetricExt,
}

impl DataSchema {
    /// The label to show for a column: the curated one, else the name.
    pub fn label_for<'a>(&'a self, column: &'a str) -> &'a str {
        self.labels.get(column).map_or(column, String::as_str)
    }

    /// All numeric columns that should be analyzed
    pub fn analyzable_columns(&self) -> Vec<String> {
        self.measure_columns.clone()
    }

    /// Check if a column is a KPI (higher priority)
    pub fn is_kpi(&self, column: &str) -> bool {
        self.kpi_columns.contains(&column.to_string())
    }
}

/// Where the detector's rules put one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    /// A date or datetime column.
    Time,
    /// A numeric column with enough distinct values to be a quantity.
    Measure,
    /// A string, categorical, or low-cardinality numeric column.
    Dimension,
    /// A string column whose name says it holds the time axis (`date`,
    /// `timestamp`, `time`): a dimension that may serve as the time column.
    TimeByName,
    /// Nothing the analysis can use.
    Unplaced,
}

/// The detector's rule for one column: temporal dtypes are time, numerics
/// are measures unless they have fewer than 20 distinct values covering under
/// 5% of rows, strings and categoricals are dimensions.
pub fn detect_column(column: &Column, row_count: usize) -> Detected {
    match column.dtype() {
        DataType::Date | DataType::Datetime(_, _) => Detected::Time,
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
            let unique_count = column.n_unique().unwrap_or(0);
            let cardinality_ratio = unique_count as f64 / row_count.max(1) as f64;
            if unique_count < 20 && cardinality_ratio < 0.05 {
                Detected::Dimension
            } else {
                Detected::Measure
            }
        },
        DataType::String => {
            let name_lower = column.name().to_lowercase();
            if name_lower == "date" || name_lower == "timestamp" || name_lower == "time" {
                Detected::TimeByName
            } else {
                Detected::Dimension
            }
        },
        other if other.is_categorical() => Detected::Dimension,
        _ => Detected::Unplaced,
    }
}

/// The detector's last resort for a time axis when no column qualified: the
/// first column, if its name mentions a date or time.
pub fn time_axis_by_first_column(df: &DataFrame) -> Option<String> {
    let first = df.get_columns().first()?.name().to_string();
    let lower = first.to_lowercase();
    (lower.contains("date") || lower.contains("time")).then_some(first)
}

/// Auto-detect schema from DataFrame (fallback when no config provided)
pub fn detect_schema(df: &DataFrame) -> Result<DataSchema> {
    let mut schema = DataSchema::empty();
    let row_count = df.height();
    for col in df.get_columns() {
        schema.place_detected(col.name(), detect_column(col, row_count));
    }
    if schema.time_column.is_none() {
        schema.time_column = time_axis_by_first_column(df);
    }
    Ok(schema)
}

impl DataSchema {
    /// No columns, default granularity.
    pub fn empty() -> Self {
        Self {
            measure_columns: Vec::new(),
            kpi_columns: Vec::new(),
            dimension_columns: Vec::new(),
            time_columns: Vec::new(),
            time_column: None,
            time_granularity: TimeGranularity::default(),
            polarity: HashMap::new(),
            entity_columns: Vec::new(),
            labels: HashMap::new(),
            descriptions: HashMap::new(),
            metrics: Vec::new(),
        }
    }

    /// Add a column where the detector's rule put it. A `TimeByName` column
    /// is a dimension that becomes the time axis if none has been chosen.
    pub fn place_detected(&mut self, name: &str, detected: Detected) {
        match detected {
            Detected::Time => {
                self.time_columns.push(name.to_string());
                if self.time_column.is_none() {
                    self.time_column = Some(name.to_string());
                }
            },
            Detected::Measure => self.measure_columns.push(name.to_string()),
            Detected::Dimension => self.dimension_columns.push(name.to_string()),
            Detected::TimeByName => {
                self.dimension_columns.push(name.to_string());
                if self.time_column.is_none() {
                    self.time_column = Some(name.to_string());
                }
            },
            Detected::Unplaced => {},
        }
    }
}

/// What the detector would declare about a table, as a producer.
///
/// One field per column with its logical type and the role the detection
/// rules give it. A string column the name rule takes for the time axis is
/// declared `is_time` explicitly, since its datatype says otherwise. Applied
/// under the `detected` layer so every other producer and every edit
/// outranks it.
pub fn detected_declaration(df: &DataFrame, table: &str) -> Result<TableDeclaration> {
    let schema = detect_schema(df)?;
    let is = |names: &[String], name: &str| names.iter().any(|n| n == name);
    let mut dataset = Dataset::new(table, table);
    for col in df.get_columns() {
        let name = col.name().to_string();
        let datatype = LogicalType::from_polars(col.dtype());
        let role = if schema.time_column.as_deref() == Some(name.as_str())
            || is(&schema.time_columns, &name)
        {
            ColumnRole::Time
        } else if is(&schema.measure_columns, &name) {
            ColumnRole::Measure
        } else if is(&schema.dimension_columns, &name) {
            ColumnRole::Dimension
        } else {
            ColumnRole::Ignored
        };
        let mut field = Field::column(name.clone())
            .with_datatype(datatype)
            .with_brightflow(&ColumnExt::role(role));
        if role == ColumnRole::Time && !datatype.is_temporal() {
            field = field.with_is_time(true);
        }
        dataset.fields.push(field);
    }
    Ok(TableDeclaration {
        dataset: Some(dataset),
        ..TableDeclaration::new(table)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_declaration_types_and_roles_every_column() {
        let df = df!(
            "order_date" => &["2024-01-01", "2024-01-02", "2024-01-03"],
            "revenue" => &[10.0, 20.0, 30.0],
            "region" => &["eu", "us", "eu"],
            "flag" => &[true, false, true],
        )
        .unwrap();
        let decl = detected_declaration(&df, "orders").unwrap();
        assert_eq!(decl.name, "orders");
        assert_eq!(decl.validate(), Ok(()));
        let ds = decl.dataset.unwrap();
        let role = |n: &str| ds.field(n).unwrap().brightflow().unwrap().role;
        assert_eq!(role("revenue"), Some(ColumnRole::Measure));
        assert_eq!(role("region"), Some(ColumnRole::Dimension));
        assert_eq!(role("flag"), Some(ColumnRole::Ignored));
        assert_eq!(
            ds.field("revenue").unwrap().datatype,
            Some(LogicalType::Float)
        );
        assert_eq!(
            ds.field("region").unwrap().datatype,
            Some(LogicalType::String)
        );
        // The first column's name carries "date": the detector's time axis,
        // declared is_time on a String column.
        assert_eq!(role("order_date"), Some(ColumnRole::Time));
        assert!(ds.field("order_date").unwrap().resolved_is_time());
        assert_eq!(
            ds.field("order_date").unwrap().datatype,
            Some(LogicalType::String)
        );
    }
}

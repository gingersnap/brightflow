//! Schema merge: combine auto-detected schema with user overrides.
//!
//! Plain Rust structs — no store dependency. Callers convert whatever rows
//! they persist into these structs before handing them in.

use anyhow::Result;
use polars::prelude::*;

use crate::data::config::{ColumnRole, Polarity, TimeGranularity};
use crate::data::schema::{detect_schema, DataSchema};

/// A user-provided column role override (from SQLite or API request).
#[derive(Debug, Clone)]
pub struct ColumnOverride {
    pub column_name: String,
    pub role: ColumnRole,
    pub is_kpi: bool,
    pub polarity: Polarity,
    pub label: Option<String>,
    pub description: Option<String>,
}

/// User-provided table-level analysis settings override.
#[derive(Debug, Clone)]
pub struct TableSettingsOverride {
    pub time_granularity: Option<TimeGranularity>,
    pub comparison_periods: Option<usize>,
}

/// Build a `DataSchema` by running auto-detection on a DataFrame, then
/// applying user overrides on top.
pub fn build_schema(
    df: &DataFrame,
    overrides: &[ColumnOverride],
    settings: Option<&TableSettingsOverride>,
) -> Result<DataSchema> {
    if overrides.is_empty() && settings.is_none() {
        return detect_schema(df);
    }

    let mut schema = detect_schema(df)?;

    // Apply column overrides: remove the column from all lists, then place it
    // in the correct one based on the override role.
    for ovr in overrides {
        let name = &ovr.column_name;

        // Remove from all current lists
        schema.measure_columns.retain(|c| c != name);
        schema.kpi_columns.retain(|c| c != name);
        schema.dimension_columns.retain(|c| c != name);
        schema.time_columns.retain(|c| c != name);
        if schema.time_column.as_deref() == Some(name) {
            schema.time_column = None;
        }

        if ovr.polarity != Polarity::Neutral {
            schema.polarity.insert(name.clone(), ovr.polarity);
        }

        match ovr.role {
            ColumnRole::Measure => {
                schema.measure_columns.push(name.clone());
                if ovr.is_kpi {
                    schema.kpi_columns.push(name.clone());
                }
            },
            ColumnRole::Dimension => {
                schema.dimension_columns.push(name.clone());
            },
            ColumnRole::Time => {
                schema.time_columns.push(name.clone());
                if schema.time_column.is_none() {
                    schema.time_column = Some(name.clone());
                }
            },
            ColumnRole::Entity | ColumnRole::Ignored => {
                // Not added to any analysis list
            },
        }
    }

    // Apply settings overrides
    if let Some(settings) = settings {
        if let Some(granularity) = settings.time_granularity {
            schema.time_granularity = granularity;
        }
    }

    Ok(schema)
}

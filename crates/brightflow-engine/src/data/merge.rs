//! Schema merge: combine auto-detected schema with user overrides.
//!
//! Plain Rust structs — no store dependency. Callers convert whatever rows
//! they persist into these structs before handing them in.

use anyhow::Result;
use polars::prelude::*;

use crate::data::config::{ColumnRole, Polarity, TimeGranularity};
use crate::data::schema::{detect_schema, DataSchema, DeclaredMetric};

/// One column's resolved semantics, as the caller read them from the store.
/// `role: None` means nobody has said what the column is for, so detection
/// places it and only the other fields apply.
#[derive(Debug, Clone)]
pub struct ColumnOverride {
    pub column_name: String,
    pub role: Option<ColumnRole>,
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
    metrics: &[DeclaredMetric],
) -> Result<DataSchema> {
    if overrides.is_empty() && settings.is_none() && metrics.is_empty() {
        return detect_schema(df);
    }

    let mut schema = detect_schema(df)?;

    // Apply column overrides: remove the column from all lists, then place it
    // in the correct one based on the override role.
    for ovr in overrides {
        let name = &ovr.column_name;

        if ovr.polarity != Polarity::Neutral {
            schema.polarity.insert(name.clone(), ovr.polarity);
        }
        if let Some(label) = ovr.label.as_deref().filter(|l| !l.is_empty()) {
            schema.labels.insert(name.clone(), label.to_string());
        }
        if let Some(description) = ovr.description.as_deref().filter(|d| !d.is_empty()) {
            schema
                .descriptions
                .insert(name.clone(), description.to_string());
        }

        // No stated role: detection's placement stands, but a KPI flag on a
        // detected measure still counts.
        let Some(role) = ovr.role else {
            if ovr.is_kpi
                && schema.measure_columns.contains(name)
                && !schema.kpi_columns.contains(name)
            {
                schema.kpi_columns.push(name.clone());
            }
            continue;
        };

        // Remove from all current lists
        schema.measure_columns.retain(|c| c != name);
        schema.kpi_columns.retain(|c| c != name);
        schema.dimension_columns.retain(|c| c != name);
        schema.time_columns.retain(|c| c != name);
        schema.entity_columns.retain(|c| c != name);
        if schema.time_column.as_deref() == Some(name) {
            schema.time_column = None;
        }

        match role {
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
            ColumnRole::Entity => {
                schema.entity_columns.push(name.clone());
            },
            ColumnRole::Ignored => {
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

    // Declared metrics: the column a metric aggregates is a measure worth
    // analysing; a KPI metric makes it a KPI; a metric's polarity applies to
    // the column unless the column has its own. The metric itself is kept for
    // the scoring context, which can name a filtered series as a KPI.
    for metric in metrics {
        let column = &metric.ext.expr.column;
        if df.column(column).is_err() {
            continue;
        }
        if !schema.measure_columns.contains(column) {
            schema.measure_columns.push(column.clone());
        }
        if metric.ext.is_kpi == Some(true) && !schema.kpi_columns.contains(column) {
            schema.kpi_columns.push(column.clone());
        }
        if let Some(polarity) = metric.ext.polarity.filter(|p| *p != Polarity::Neutral) {
            schema.polarity.entry(column.clone()).or_insert(polarity);
        }
        schema.metrics.push(metric.clone());
    }

    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A declared KPI metric makes its column a measure and a KPI, and its
    /// polarity applies unless the column has one of its own.
    #[test]
    fn declared_metrics_make_their_column_a_kpi_measure() {
        use brightflow_types::{Aggregation, MetricExpr, MetricExt};
        let df = polars::df!(
            "region" => &["eu", "us", "eu"],
            "reactions_total" => &[1, 2, 3],
            "churn" => &[1.0, 2.0, 3.0],
        )
        .unwrap();
        let metric = |column: &str, polarity: Option<Polarity>| DeclaredMetric {
            name: format!("{column}_metric"),
            ext: MetricExt {
                expr: MetricExpr {
                    dataset: None,
                    column: column.to_string(),
                    aggregation: Aggregation::Sum,
                    filters: vec![],
                },
                is_kpi: Some(true),
                polarity,
                format: None,
            },
        };
        let overrides = vec![ColumnOverride {
            column_name: "churn".to_string(),
            role: Some(ColumnRole::Measure),
            is_kpi: false,
            polarity: Polarity::LowerIsBetter,
            label: None,
            description: None,
        }];
        let schema = build_schema(
            &df,
            &overrides,
            None,
            &[
                metric("reactions_total", Some(Polarity::HigherIsBetter)),
                metric("churn", Some(Polarity::HigherIsBetter)),
                metric("missing", None),
            ],
        )
        .unwrap();
        assert!(schema.kpi_columns.contains(&"reactions_total".to_string()));
        assert!(schema.kpi_columns.contains(&"churn".to_string()));
        assert_eq!(schema.polarity["reactions_total"], Polarity::HigherIsBetter);
        // The column's own polarity wins over the metric's.
        assert_eq!(schema.polarity["churn"], Polarity::LowerIsBetter);
        // A metric over a column the frame lacks is dropped.
        assert_eq!(schema.metrics.len(), 2);
    }
}

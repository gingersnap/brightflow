//! Schema merge: the resolved semantic rows first, the detector for the rest.
//!
//! The store's resolved rows are the schema source. A column with a resolved
//! role is placed by it; a column no row mentions goes through the detector's
//! per-column rule, the same rule the `detected` layer was written with, so
//! a table that has been detected is never re-detected here. A table with no
//! rows at all is detected whole, exactly as `detect_schema` does. Labels,
//! descriptions and polarity come from the rows regardless of role; the
//! table's own row supplies the period; declared metrics make their column a
//! measure and, when they are KPIs, a KPI.

use anyhow::Result;
use brightflow_types::{ResolvedColumn, ResolvedTable};
use polars::prelude::*;

use crate::data::config::{ColumnRole, Polarity};
use crate::data::schema::{
    detect_column, detect_schema, time_axis_by_first_column, DataSchema, DeclaredMetric, Detected,
};

/// Build a `DataSchema` from the frame's columns, the store's resolved rows,
/// the table's resolved settings and its declared metrics.
pub fn build_schema(
    df: &DataFrame,
    resolved: &[ResolvedColumn],
    table: Option<&ResolvedTable>,
    metrics: &[DeclaredMetric],
) -> Result<DataSchema> {
    if resolved.is_empty() && table.is_none() && metrics.is_empty() {
        return detect_schema(df);
    }

    let mut schema = DataSchema::empty();
    let row_count = df.height();
    // The first resolved time column is the axis, even when a roleless
    // column placed earlier by name already claimed it.
    let mut resolved_time_axis = false;
    for col in df.get_columns() {
        let name = col.name().as_str();
        let row = resolved.iter().find(|r| r.name == name);
        if let Some(row) = row {
            if let Some(polarity) = row.polarity.filter(|p| *p != Polarity::Neutral) {
                schema.polarity.insert(name.to_string(), polarity);
            }
            if let Some(label) = row.label.as_deref().filter(|l| !l.is_empty()) {
                schema.labels.insert(name.to_string(), label.to_string());
            }
            if let Some(description) = row.description.as_deref().filter(|d| !d.is_empty()) {
                schema
                    .descriptions
                    .insert(name.to_string(), description.to_string());
            }
        }
        match row.and_then(|r| r.role) {
            Some(ColumnRole::Measure) => {
                schema.measure_columns.push(name.to_string());
                if row.is_some_and(|r| r.is_kpi == Some(true)) {
                    schema.kpi_columns.push(name.to_string());
                }
            },
            Some(ColumnRole::Dimension) => schema.dimension_columns.push(name.to_string()),
            Some(ColumnRole::Time) => {
                schema.time_columns.push(name.to_string());
                if !resolved_time_axis {
                    schema.time_column = Some(name.to_string());
                    resolved_time_axis = true;
                }
            },
            Some(ColumnRole::Entity) => schema.entity_columns.push(name.to_string()),
            Some(ColumnRole::Ignored) => {},
            // No stated role: the detector places it. A KPI flag on a
            // detected measure still counts.
            None => {
                let detected = detect_column(col, row_count);
                schema.place_detected(name, detected);
                if detected == Detected::Measure && row.is_some_and(|r| r.is_kpi == Some(true)) {
                    schema.kpi_columns.push(name.to_string());
                }
            },
        }
    }
    if schema.time_column.is_none() {
        schema.time_column = time_axis_by_first_column(df);
    }

    if let Some(granularity) = table.and_then(|t| t.time_granularity) {
        schema.time_granularity = granularity;
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
    use brightflow_types::TimeGranularity;

    fn resolved(name: &str, role: Option<ColumnRole>) -> ResolvedColumn {
        ResolvedColumn {
            name: name.to_string(),
            role,
            ..ResolvedColumn::default()
        }
    }

    fn frame() -> DataFrame {
        polars::df!(
            "order_date" => &["2024-01-01", "2024-01-02", "2024-01-03"],
            "region" => &["eu", "us", "eu"],
            "year" => &[2024_i64, 2024, 2024],
            "revenue" => &[10.0, 20.0, 30.0],
            "unmentioned" => &[1.5, 2.5, 3.5],
        )
        .unwrap()
    }

    /// A resolved role wins over what detection would say; a column no row
    /// mentions is placed by detection; labels, descriptions and polarity
    /// ride along; the table's row sets the period.
    #[test]
    fn resolved_rows_place_columns_and_detection_covers_the_rest() {
        let mut year = resolved("year", Some(ColumnRole::Dimension));
        year.description = Some("Fiscal year".into());
        let mut revenue = resolved("revenue", Some(ColumnRole::Measure));
        revenue.is_kpi = Some(true);
        revenue.polarity = Some(Polarity::HigherIsBetter);
        revenue.label = Some("Revenue (SEK)".into());
        let table = ResolvedTable {
            time_granularity: Some(TimeGranularity::Month),
            ..ResolvedTable::default()
        };
        let schema = build_schema(
            &frame(),
            &[
                resolved("order_date", Some(ColumnRole::Time)),
                year,
                revenue,
                resolved("region", None),
            ],
            Some(&table),
            &[],
        )
        .unwrap();
        // `year` is numeric with one distinct value: detection would call it a
        // dimension too, but a measure-looking `revenue`-style column resolved
        // as a dimension must also land there, so pin the resolved placement.
        assert_eq!(schema.dimension_columns, ["region", "year"]);
        assert_eq!(schema.measure_columns, ["revenue", "unmentioned"]);
        assert_eq!(schema.kpi_columns, ["revenue"]);
        assert_eq!(schema.time_column.as_deref(), Some("order_date"));
        assert_eq!(schema.time_columns, ["order_date"]);
        assert_eq!(schema.polarity["revenue"], Polarity::HigherIsBetter);
        assert_eq!(schema.labels["revenue"], "Revenue (SEK)");
        assert_eq!(schema.descriptions["year"], "Fiscal year");
        assert_eq!(schema.time_granularity, TimeGranularity::Month);
    }

    /// A numeric column detection would call a measure, resolved as a
    /// dimension, is a dimension.
    #[test]
    fn a_resolved_dimension_beats_detection_on_a_numeric_column() {
        let schema = build_schema(
            &frame(),
            &[resolved("revenue", Some(ColumnRole::Dimension))],
            None,
            &[],
        )
        .unwrap();
        assert!(schema.dimension_columns.contains(&"revenue".to_string()));
        assert!(!schema.measure_columns.contains(&"revenue".to_string()));
    }

    /// A row with no role leaves placement to detection; its KPI flag counts
    /// only on a detected measure.
    #[test]
    fn a_roleless_row_is_placed_by_detection() {
        let mut kpi = resolved("unmentioned", None);
        kpi.is_kpi = Some(true);
        let mut kpi_on_dimension = resolved("region", None);
        kpi_on_dimension.is_kpi = Some(true);
        let schema = build_schema(&frame(), &[kpi, kpi_on_dimension], None, &[]).unwrap();
        assert_eq!(schema.kpi_columns, ["unmentioned"]);
        assert!(schema.dimension_columns.contains(&"region".to_string()));
    }

    /// Nothing resolved at all: the whole-frame detector, unchanged.
    #[test]
    fn no_rows_means_plain_detection() {
        let plain = build_schema(&frame(), &[], None, &[]).unwrap();
        let detected = detect_schema(&frame()).unwrap();
        assert_eq!(plain.measure_columns, detected.measure_columns);
        assert_eq!(plain.dimension_columns, detected.dimension_columns);
        assert_eq!(plain.time_column, detected.time_column);
        // The first column's name carries "date": the name rule picks it.
        assert_eq!(plain.time_column.as_deref(), Some("order_date"));
    }

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
        let mut churn = resolved("churn", Some(ColumnRole::Measure));
        churn.polarity = Some(Polarity::LowerIsBetter);
        let schema = build_schema(
            &df,
            &[churn],
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

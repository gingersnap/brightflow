//! Executes a `Query` operation chain against a dataset.
//!
//! Everything is built as a Polars `LazyFrame` and collected once at the end, so
//! filters and projections push down into the Parquet scan instead of
//! materializing the whole table first. That is what makes store-backed datasets
//! (which are only file paths) queryable without loading them.

use crate::analytics::session::{ColumnInfo, DatasetData};
use crate::analytics::types::{
    AggSpec, Aggregation, DerivedColumn, DerivedExpr, FilterOp, Operation, Query, QueryResponse,
};
use crate::shared::{AppError, AppResult};
use brightflow_types::TimeGranularity;
use polars::prelude::*;
use std::time::Instant;

/// Execute a query against dataset data
pub fn execute_query(data: &DatasetData, query: Query) -> AppResult<QueryResponse> {
    let start = Instant::now();

    // Build LazyFrame from the data source
    let mut lf = match data {
        DatasetData::Uploaded(df) => df.clone().lazy(),
        DatasetData::Parquet { files } => {
            LazyFrame::scan_parquet_files(files.clone().into(), ScanArgsParquet::default())?
        },
    };

    // Separate Limit from other operations to get correct total_rows
    let mut limit_op: Option<u32> = None;
    let mut other_ops = Vec::new();

    for op in query.operations {
        if let Operation::Limit { n } = op {
            limit_op = Some(n);
        } else {
            other_ops.push(op);
        }
    }

    // Apply all operations except Limit
    for op in other_ops {
        lf = apply_operation(lf, op)?;
    }

    // Collect results with two-pass approach when limit is present
    let (result_df, total_rows) = if let Some(n) = limit_op {
        // 1) Count rows (Polars optimizes to metadata scan when unfiltered)
        let total_rows = lf
            .clone()
            .select([len()])
            .collect()?
            .column("len")?
            .u32()?
            .get(0)
            .unwrap_or(0) as usize;
        // 2) Only materialize the limited rows
        let result_df = lf.limit(n).collect()?;
        (result_df, total_rows)
    } else {
        let filtered_df = lf.collect()?;
        let total_rows = filtered_df.height();
        (filtered_df, total_rows)
    };

    // Convert to response
    let columns = extract_column_info(&result_df);
    let rows = df_to_json_rows(&result_df)?;
    let row_count = result_df.height();

    // request_id is echoed by the WS layer; the REST path leaves it absent.
    Ok(QueryResponse {
        columns,
        rows,
        row_count,
        total_rows,
        execution_time_ms: start.elapsed().as_secs_f64() * 1000.0,
        request_id: None,
    })
}

/// Apply a single operation to a LazyFrame
fn apply_operation(mut lf: LazyFrame, op: Operation) -> AppResult<LazyFrame> {
    match op {
        Operation::Filter { column, op, value } => {
            // The plan's schema decides how a string value is compared: a
            // date column gets a date literal, not text.
            let dtype = lf.collect_schema()?.get(column.as_str()).cloned();
            let expr = build_filter_expr(&column, dtype.as_ref(), op, value)?;
            Ok(lf.filter(expr))
        },
        Operation::Select { columns } => {
            let cols: Vec<Expr> = columns.iter().map(col).collect();
            Ok(lf.select(cols))
        },
        Operation::GroupBy { by, aggs } => {
            let by_exprs: Vec<Expr> = by.iter().map(col).collect();
            let agg_exprs: Vec<Expr> = aggs.iter().map(build_agg_expr).collect();
            Ok(lf.group_by(by_exprs).agg(agg_exprs))
        },
        Operation::Pivot {
            index,
            columns,
            values,
            agg,
        } => apply_pivot(lf, index, columns, values, agg),
        Operation::Sort { by, descending } => Ok(lf.sort(
            [&by],
            SortMultipleOptions::default().with_order_descending(descending),
        )),
        Operation::Limit { n } => Ok(lf.limit(n)),
        Operation::WithColumns { columns } => apply_with_columns(lf, columns),
    }
}

/// Add derived columns. The plan's schema is resolved once (no data is read)
/// because the expression for a column depends on its dtype.
fn apply_with_columns(mut lf: LazyFrame, columns: Vec<DerivedColumn>) -> AppResult<LazyFrame> {
    let schema = lf.collect_schema()?;
    let exprs = columns
        .into_iter()
        .map(|column| build_derived_expr(&schema, column))
        .collect::<AppResult<Vec<Expr>>>()?;
    Ok(lf.with_columns(exprs))
}

fn build_derived_expr(schema: &Schema, column: DerivedColumn) -> AppResult<Expr> {
    match column.expr {
        DerivedExpr::Period {
            column: source,
            granularity,
        } => {
            let dtype = schema.get(source.as_str()).ok_or_else(|| {
                AppError::InvalidQuery(format!("column '{source}' not found for period"))
            })?;
            Ok(period_expr(&source, dtype, granularity)?.alias(column.name.as_str()))
        },
    }
}

/// The engine's period label for a time column at a granularity. Strings are
/// read as ISO dates by their first ten characters, non-strict, so a value
/// that is not a date buckets to null rather than failing the query. The
/// label formats must stay identical to the engine's `format_period`; a test
/// pins them against it.
fn period_expr(column: &str, dtype: &DataType, granularity: TimeGranularity) -> AppResult<Expr> {
    let date = match dtype {
        DataType::String => {
            col(column)
                .str()
                .slice(lit(0), lit(10))
                .str()
                .to_date(StrptimeOptions {
                    format: Some("%Y-%m-%d".into()),
                    strict: false,
                    exact: true,
                    cache: true,
                })
        },
        DataType::Date => col(column),
        DataType::Datetime(_, _) => col(column).cast(DataType::Date),
        other => {
            return Err(AppError::InvalidQuery(format!(
                "column '{column}' has type {other}, which cannot be bucketed by period"
            )))
        },
    };
    Ok(match granularity {
        TimeGranularity::Day => date.dt().strftime("%Y-%m-%d"),
        // ISO week-year and week number, the same pair the engine uses.
        TimeGranularity::Week => date.dt().strftime("%G-W%V"),
        TimeGranularity::Month => date.dt().strftime("%Y-%m"),
        // No strftime code for the quarter; `+` on String expressions
        // concatenates, so no extra Polars feature is needed.
        TimeGranularity::Quarter => {
            date.clone().dt().year().cast(DataType::String)
                + lit("-Q")
                + date.dt().quarter().cast(DataType::String)
        },
        TimeGranularity::Year => date.dt().strftime("%Y"),
    })
}

/// A string value against a temporal column, as the comparison Polars can
/// run: the column and the literal to compare it with. A date-only string
/// against a datetime column compares by calendar day, which is what
/// "before 2024-03-01" means to a person. `None` when the column is not
/// temporal or the value is not a string, so the plain literal applies.
fn temporal_comparison(column: &str, dtype: &DataType, value: &str) -> Option<(Expr, Expr)> {
    let text = value.trim();
    let date_options = StrptimeOptions {
        format: Some("%Y-%m-%d".into()),
        strict: false,
        exact: true,
        cache: true,
    };
    let is_date_only = text.len() == 10;
    match dtype {
        DataType::Date => Some((col(column), lit(text).str().to_date(date_options))),
        DataType::Datetime(unit, zone) => {
            if is_date_only {
                return Some((
                    col(column).cast(DataType::Date),
                    lit(text).str().to_date(date_options),
                ));
            }
            // `datetime-local` inputs omit seconds; strptime wants them.
            let full = if text.len() == 16 {
                format!("{text}:00")
            } else {
                text.to_string()
            };
            let literal = lit(full).str().to_datetime(
                Some(*unit),
                zone.clone(),
                StrptimeOptions {
                    format: None,
                    strict: false,
                    exact: true,
                    cache: true,
                },
                lit("raise"),
            );
            Some((col(column), literal))
        },
        DataType::Time => Some((
            col(column),
            lit(text).str().to_time(StrptimeOptions {
                format: None,
                strict: false,
                exact: true,
                cache: true,
            }),
        )),
        _ => None,
    }
}

/// Build a filter expression. `dtype` is the column's planned type when the
/// caller resolved it; it only changes how string values are compared with
/// temporal columns.
fn build_filter_expr(
    column: &str,
    dtype: Option<&DataType>,
    op: FilterOp,
    value: serde_json::Value,
) -> AppResult<Expr> {
    let c = col(column);
    let temporal = match (&value, dtype) {
        (serde_json::Value::String(s), Some(dtype)) => temporal_comparison(column, dtype, s),
        _ => None,
    };
    if let Some((column_expr, literal)) = temporal {
        let comparison = match op {
            FilterOp::Eq => Some(column_expr.eq(literal)),
            FilterOp::Ne => Some(column_expr.neq(literal)),
            FilterOp::Gt => Some(column_expr.gt(literal)),
            FilterOp::Gte => Some(column_expr.gt_eq(literal)),
            FilterOp::Lt => Some(column_expr.lt(literal)),
            FilterOp::Lte => Some(column_expr.lt_eq(literal)),
            _ => None,
        };
        if let Some(expr) = comparison {
            return Ok(expr);
        }
    }

    Ok(match op {
        FilterOp::Eq => c.eq(json_to_lit(&value)?),
        FilterOp::Ne => c.neq(json_to_lit(&value)?),
        FilterOp::Gt => c.gt(json_to_lit(&value)?),
        FilterOp::Gte => c.gt_eq(json_to_lit(&value)?),
        FilterOp::Lt => c.lt(json_to_lit(&value)?),
        FilterOp::Lte => c.lt_eq(json_to_lit(&value)?),
        FilterOp::Contains => {
            let s = value
                .as_str()
                .ok_or_else(|| AppError::InvalidQuery("Contains requires string value".into()))?;
            c.cast(DataType::String)
                .str()
                .contains_literal(lit(s.to_string()))
        },
        FilterOp::In => {
            let arr = value
                .as_array()
                .ok_or_else(|| AppError::InvalidQuery("In requires array value".into()))?;

            // Build OR chain for "in" operation
            if arr.is_empty() {
                lit(false)
            } else {
                let mut expr = c.clone().eq(json_to_lit(&arr[0])?);
                for v in &arr[1..] {
                    expr = expr.or(c.clone().eq(json_to_lit(v)?));
                }
                expr
            }
        },
        FilterOp::IsNull => c.is_null(),
        FilterOp::IsNotNull => c.is_not_null(),
    })
}

/// Build an aggregation expression
fn build_agg_expr(spec: &AggSpec) -> Expr {
    let base = if spec.column == "*" {
        len()
    } else {
        col(&spec.column)
    };

    let agg = match spec.function {
        Aggregation::Count => {
            if spec.column == "*" {
                base // len() already returns count
            } else {
                base.count()
            }
        },
        Aggregation::Sum => base.sum(),
        Aggregation::Avg => base.mean(),
        Aggregation::Min => base.min(),
        Aggregation::Max => base.max(),
        Aggregation::Median => base.median(),
        Aggregation::Std => base.std(1),
        Aggregation::First => base.first(),
        Aggregation::Last => base.last(),
    };

    match &spec.alias {
        Some(alias) => agg.alias(alias),
        None => agg,
    }
}

/// Apply a pivot operation: `index` stays as the leading row columns, each
/// distinct value of `columns` becomes a column, `values` fills the cells.
///
/// Polars' `pivot` takes `on` (what spreads into columns) *before* `index`
/// (what stays as rows). Passing the wire fields in their own order
/// transposes the result, which is how this once shipped with rows and
/// columns swapped; the test below pins the orientation.
fn apply_pivot(
    lf: LazyFrame,
    index: Vec<String>,
    columns: String,
    values: String,
    agg: Option<Aggregation>,
) -> AppResult<LazyFrame> {
    // Pivot requires collecting to DataFrame first
    let temp_df = lf.collect()?;

    // Convert aggregation to expression
    let agg_expr = agg.map(|a| to_polars_agg_expr(&values, a));

    let pivoted = pivot::pivot(
        &temp_df,
        [columns.as_str()],
        Some(index.iter().map(String::as_str)),
        Some([values.as_str()]),
        false,
        agg_expr,
        None,
    )?;

    Ok(pivoted.lazy())
}

/// Convert aggregation enum to Polars expression for pivot
fn to_polars_agg_expr(values_col: &str, agg: Aggregation) -> Expr {
    let base = col(values_col);
    match agg {
        Aggregation::Count => base.count(),
        Aggregation::Sum => base.sum(),
        Aggregation::Avg => base.mean(),
        Aggregation::Min => base.min(),
        Aggregation::Max => base.max(),
        Aggregation::First => base.first(),
        Aggregation::Last => base.last(),
        Aggregation::Median => base.median(),
        Aggregation::Std => base.std(1),
    }
}

/// Convert a JSON value to a Polars Literal expression
fn json_to_lit(value: &serde_json::Value) -> AppResult<Expr> {
    Ok(match value {
        serde_json::Value::Null => lit(NULL),
        serde_json::Value::Bool(b) => lit(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                lit(i)
            } else if let Some(f) = n.as_f64() {
                lit(f)
            } else {
                return Err(AppError::InvalidQuery("Invalid number value".into()));
            }
        },
        serde_json::Value::String(s) => lit(s.clone()),
        _ => return Err(AppError::InvalidQuery("Unsupported value type".into())),
    })
}

/// Extract column info from a DataFrame
fn extract_column_info(df: &DataFrame) -> Vec<ColumnInfo> {
    df.get_columns()
        .iter()
        .map(|col| ColumnInfo::plain(col.name(), col.dtype()))
        .collect()
}

/// Convert DataFrame rows to JSON
fn df_to_json_rows(df: &DataFrame) -> AppResult<Vec<Vec<serde_json::Value>>> {
    let mut rows = Vec::with_capacity(df.height());

    for i in 0..df.height() {
        let mut row = Vec::with_capacity(df.width());
        for col in df.get_columns() {
            let val = col.get(i)?;
            row.push(anyvalue_to_json(&val));
        }
        rows.push(row);
    }

    Ok(rows)
}

/// Convert a Polars AnyValue to JSON
fn anyvalue_to_json(val: &AnyValue<'_>) -> serde_json::Value {
    match val {
        AnyValue::Null => serde_json::Value::Null,
        AnyValue::Boolean(b) => serde_json::Value::Bool(*b),
        AnyValue::Int8(n) => serde_json::json!(*n),
        AnyValue::Int16(n) => serde_json::json!(*n),
        AnyValue::Int32(n) => serde_json::json!(*n),
        AnyValue::Int64(n) => serde_json::json!(*n),
        AnyValue::UInt8(n) => serde_json::json!(*n),
        AnyValue::UInt16(n) => serde_json::json!(*n),
        AnyValue::UInt32(n) => serde_json::json!(*n),
        AnyValue::UInt64(n) => serde_json::json!(*n),
        AnyValue::Float32(n) => serde_json::json!(*n),
        AnyValue::Float64(n) => serde_json::json!(*n),
        AnyValue::String(s) => serde_json::Value::String(s.to_string()),
        AnyValue::StringOwned(s) => serde_json::Value::String(s.to_string()),
        _ => serde_json::Value::String(format!("{val:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A string value against a Date column filters by date, not by text;
    /// a date-only string against a Datetime column filters by calendar
    /// day; a full timestamp compares exactly.
    #[test]
    fn temporal_filters_compare_dates_not_text() {
        use polars::prelude::NamedFrom;
        let days = Series::new("d".into(), &[19737_i32, 19738, 19800])
            .cast(&DataType::Date)
            .unwrap();
        let stamps = Series::new(
            "ts".into(),
            &[
                1_705_314_600_000_000_i64,
                1_705_400_000_000_000,
                1_710_000_000_000_000,
            ],
        )
        .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
        .unwrap();
        let lf = DataFrame::new(vec![days.into(), stamps.into()])
            .unwrap()
            .lazy();
        let filter = |column: &str, op: FilterOp, value: &str| {
            apply_operation(
                lf.clone(),
                Operation::Filter {
                    column: column.to_string(),
                    op,
                    value: serde_json::Value::String(value.to_string()),
                },
            )
            .unwrap()
            .collect()
            .unwrap()
            .height()
        };
        // 19737 days = 2024-01-15.
        assert_eq!(filter("d", FilterOp::Gte, "2024-01-16"), 2);
        assert_eq!(filter("d", FilterOp::Lt, "2024-01-16"), 1);
        assert_eq!(filter("d", FilterOp::Eq, "2024-01-15"), 1);
        // The first stamp is 2024-01-15T10:30:00; the second is the next day.
        assert_eq!(filter("ts", FilterOp::Eq, "2024-01-15"), 1);
        assert_eq!(filter("ts", FilterOp::Gt, "2024-01-15T11:00"), 2);
        assert_eq!(filter("ts", FilterOp::Lte, "2024-01-15T10:30:00"), 1);
    }

    fn tickets() -> LazyFrame {
        df! {
            "id" => [1, 2, 3, 4, 5, 6],
            "category" => ["a", "a", "a", "b", "b", "b"],
            "subcategory" => ["x", "x", "y", "y", "z", "z"],
            "language" => ["en", "sv", "en", "en", "en", "sv"],
        }
        .unwrap()
        .lazy()
    }

    fn names(df: &DataFrame) -> Vec<String> {
        df.get_column_names_str()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// Rows are the index, column-field values spread across — never the
    /// other way round.
    #[test]
    fn pivot_keeps_index_as_rows_and_spreads_the_column_field() {
        let out = apply_pivot(
            tickets(),
            vec!["category".to_string()],
            "subcategory".to_string(),
            "id".to_string(),
            Some(Aggregation::Count),
        )
        .unwrap()
        .collect()
        .unwrap();
        assert_eq!(names(&out), vec!["category", "x", "y", "z"]);
        assert_eq!(out.height(), 2);
        let x = out.column("x").unwrap().u32().unwrap();
        let y = out.column("y").unwrap().u32().unwrap();
        // a: x=2, y=1, z=none; b: x=none, y=1, z=2
        assert_eq!(x.get(0), Some(2));
        assert_eq!(y.get(0), Some(1));
        assert_eq!(x.get(1), None);
        assert_eq!(y.get(1), Some(1));
    }

    #[test]
    fn pivot_with_two_index_columns_keeps_both_leading() {
        let out = apply_pivot(
            tickets(),
            vec!["category".to_string(), "language".to_string()],
            "subcategory".to_string(),
            "id".to_string(),
            Some(Aggregation::Count),
        )
        .unwrap()
        .collect()
        .unwrap();
        assert_eq!(names(&out), vec!["category", "language", "x", "y", "z"]);
        assert_eq!(out.height(), 4);
    }

    /// Counting the column field itself (Columns = subcategory, Values =
    /// count of subcategory) is what a user clicking through the buckets
    /// naturally builds; pin what Polars does with it so the UI can rely
    /// on it.
    #[test]
    fn pivot_can_count_the_column_field_itself() {
        let out = apply_pivot(
            tickets(),
            vec!["category".to_string()],
            "subcategory".to_string(),
            "subcategory".to_string(),
            Some(Aggregation::Count),
        )
        .unwrap()
        .collect()
        .unwrap();
        assert_eq!(names(&out), vec!["category", "x", "y", "z"]);
        let x = out.column("x").unwrap().u32().unwrap();
        assert_eq!(x.get(0), Some(2));
    }

    /// ISO datetimes with and without offsets, a plain date, and junk — the
    /// shape every time column in the product has today (all Strings).
    fn stamps() -> LazyFrame {
        df! {
            "ts" => [
                "2024-12-30T10:00:00Z",
                "2025-01-01",
                "2024-03-15T00:00:00+02:00",
                "bogus",
            ],
            "n" => [1, 2, 3, 4],
        }
        .unwrap()
        .lazy()
    }

    fn periods(lf: LazyFrame, column: &str, granularity: TimeGranularity) -> Vec<Option<String>> {
        let out = apply_with_columns(
            lf,
            vec![DerivedColumn {
                name: "p".to_string(),
                expr: DerivedExpr::Period {
                    column: column.to_string(),
                    granularity,
                },
            }],
        )
        .unwrap()
        .collect()
        .unwrap();
        out.column("p")
            .unwrap()
            .str()
            .unwrap()
            .into_iter()
            .map(|v| v.map(str::to_string))
            .collect()
    }

    #[test]
    fn period_labels_strings_at_every_granularity_in_the_engine_format() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(
            periods(stamps(), "ts", TimeGranularity::Day),
            vec![s("2024-12-30"), s("2025-01-01"), s("2024-03-15"), None]
        );
        // 2024-12-30 is a Monday of ISO week 1 of 2025.
        assert_eq!(
            periods(stamps(), "ts", TimeGranularity::Week),
            vec![s("2025-W01"), s("2025-W01"), s("2024-W11"), None]
        );
        assert_eq!(
            periods(stamps(), "ts", TimeGranularity::Month),
            vec![s("2024-12"), s("2025-01"), s("2024-03"), None]
        );
        assert_eq!(
            periods(stamps(), "ts", TimeGranularity::Quarter),
            vec![s("2024-Q4"), s("2025-Q1"), s("2024-Q1"), None]
        );
        assert_eq!(
            periods(stamps(), "ts", TimeGranularity::Year),
            vec![s("2024"), s("2025"), s("2024"), None]
        );
    }

    /// A typed Date column gives the same labels as its ISO string form, and
    /// both match what the engine writes for the same dates.
    #[test]
    fn period_labels_agree_between_date_columns_and_the_engine() {
        let dated = stamps()
            .with_column(
                col("ts")
                    .str()
                    .slice(lit(0), lit(10))
                    .str()
                    .to_date(StrptimeOptions {
                        format: Some("%Y-%m-%d".into()),
                        strict: false,
                        exact: true,
                        cache: false,
                    })
                    .alias("d"),
            )
            .collect()
            .unwrap();
        for granularity in TimeGranularity::ALL {
            let from_date = periods(dated.clone().lazy(), "d", granularity);
            let from_string = periods(dated.clone().lazy(), "ts", granularity);
            assert_eq!(from_date, from_string, "{granularity:?}");
            let engine = brightflow_engine::analysis::period::get_period_labels(
                dated.column("d").unwrap(),
                granularity,
            )
            .unwrap();
            assert_eq!(
                from_date, engine,
                "{granularity:?} disagrees with the engine"
            );
        }
    }

    #[test]
    fn period_on_a_numeric_column_is_an_invalid_query() {
        let err = apply_with_columns(
            stamps(),
            vec![DerivedColumn {
                name: "p".to_string(),
                expr: DerivedExpr::Period {
                    column: "n".to_string(),
                    granularity: TimeGranularity::Month,
                },
            }],
        )
        .err()
        .unwrap();
        assert!(matches!(err, AppError::InvalidQuery(_)), "{err}");
        let missing = apply_with_columns(
            stamps(),
            vec![DerivedColumn {
                name: "p".to_string(),
                expr: DerivedExpr::Period {
                    column: "nope".to_string(),
                    granularity: TimeGranularity::Month,
                },
            }],
        )
        .err()
        .unwrap();
        assert!(matches!(missing, AppError::InvalidQuery(_)), "{missing}");
    }

    /// The derived name groups like any column: a month bucket before a
    /// group-by yields one row per month.
    #[test]
    fn a_period_column_can_be_grouped_on() {
        let lf = apply_with_columns(
            stamps(),
            vec![DerivedColumn {
                name: "ts__month".to_string(),
                expr: DerivedExpr::Period {
                    column: "ts".to_string(),
                    granularity: TimeGranularity::Month,
                },
            }],
        )
        .unwrap();
        let out = apply_operation(
            lf,
            Operation::GroupBy {
                by: vec!["ts__month".to_string()],
                aggs: vec![AggSpec {
                    column: "n".to_string(),
                    function: Aggregation::Sum,
                    alias: Some("sum".to_string()),
                }],
            },
        )
        .unwrap()
        .sort(["ts__month"], SortMultipleOptions::default())
        .collect()
        .unwrap();
        assert_eq!(out.height(), 4, "three months plus the null bucket");
        assert_eq!(names(&out), vec!["ts__month", "sum"]);
    }
}

use crate::analytics::session::{ColumnInfo, DatasetData};
use crate::analytics::types::{AggSpec, Aggregation, FilterOp, Operation, Query, QueryResponse};
use crate::shared::{AppError, AppResult};
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

    Ok(QueryResponse {
        columns,
        rows,
        row_count,
        total_rows,
        execution_time_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Apply a single operation to a LazyFrame
fn apply_operation(lf: LazyFrame, op: Operation) -> AppResult<LazyFrame> {
    match op {
        Operation::Filter { column, op, value } => {
            let expr = build_filter_expr(&column, op, value)?;
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
    }
}

/// Build a filter expression
fn build_filter_expr(column: &str, op: FilterOp, value: serde_json::Value) -> AppResult<Expr> {
    let c = col(column);

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

/// Apply a pivot operation
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
        index.iter().map(String::as_str),
        Some([columns.as_str()]),
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
        .map(|col| ColumnInfo {
            name: col.name().to_string(),
            dtype: dtype_to_string(col.dtype()),
            role: None,
            is_kpi: None,
            label: None,
        })
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

/// Convert Polars DataType to a display string
fn dtype_to_string(dtype: &DataType) -> String {
    match dtype {
        DataType::Boolean => "bool",
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64 => "int",
        DataType::Float32 | DataType::Float64 => "float",
        DataType::String => "string",
        DataType::Datetime(_, _) => "datetime",
        DataType::Date => "date",
        DataType::Time => "time",
        DataType::Duration(_) => "duration",
        DataType::Null => "null",
        DataType::List(_) => "list",
        DataType::Struct(_) => "struct",
        _ => "unknown",
    }
    .to_string()
}

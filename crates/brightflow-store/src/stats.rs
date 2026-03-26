//! Column-level statistics extraction from DataFrames

use polars::prelude::*;

use crate::models::ColumnStatRow;

/// Extract column-level statistics (min, max, null_count) from a DataFrame.
///
/// All values are stored as string representations for heterogeneous type storage.
pub fn extract_column_stats(df: &DataFrame, table_id: &str) -> Vec<ColumnStatRow> {
    let mut stats = Vec::with_capacity(df.width());

    for col in df.get_columns() {
        let name = col.name().to_string();
        let null_count = i64::try_from(col.null_count()).ok();

        let (min_value, max_value) = if supports_min_max(col.dtype()) {
            let min_scalar = col.min_reduce().ok();
            let max_scalar = col.max_reduce().ok();
            (
                min_scalar.and_then(|s| format_scalar(&s)),
                max_scalar.and_then(|s| format_scalar(&s)),
            )
        } else {
            (None, None)
        };

        stats.push(ColumnStatRow {
            table_id: table_id.to_string(),
            column_name: name,
            min_value,
            max_value,
            null_count,
        });
    }

    stats
}

/// Check if a DataType supports min/max reduction
fn supports_min_max(dtype: &DataType) -> bool {
    matches!(
        dtype,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float32
            | DataType::Float64
            | DataType::Date
            | DataType::Datetime(_, _)
            | DataType::Duration(_)
            | DataType::String
            | DataType::Boolean
    )
}

/// Format a Polars Scalar to an Option<String> for storage
fn format_scalar(scalar: &Scalar) -> Option<String> {
    let av = scalar.value();
    if matches!(av, AnyValue::Null) {
        return None;
    }
    Some(format!("{av}"))
}

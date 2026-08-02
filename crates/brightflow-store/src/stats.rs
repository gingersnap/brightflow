//! Column-level statistics extraction from DataFrames

use polars::prelude::*;

use crate::models::{ColumnStatRow, FileColumnStatRow};

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

/// Extract per-file column statistics (min, max, null_count) from a DataFrame.
///
/// Same logic as `extract_column_stats` but returns `FileColumnStatRow` with a `file_id` field.
pub fn extract_file_column_stats(df: &DataFrame, file_id: &str) -> Vec<FileColumnStatRow> {
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

        stats.push(FileColumnStatRow {
            file_id: file_id.to_string(),
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

/// Format a Polars Scalar to an Option<String> for storage.
///
/// Polars' Display trait wraps strings in quotes — extract raw values instead.
fn format_scalar(scalar: &Scalar) -> Option<String> {
    match scalar.value() {
        AnyValue::Null => None,
        AnyValue::String(s) => Some(s.to_string()),
        AnyValue::StringOwned(s) => Some(s.to_string()),
        av => Some(format!("{av}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_max_supported_for_ordered_scalar_types() {
        assert!(supports_min_max(&DataType::Int64));
        assert!(supports_min_max(&DataType::Float64));
        assert!(supports_min_max(&DataType::String));
        assert!(supports_min_max(&DataType::Date));
        assert!(supports_min_max(&DataType::Datetime(
            TimeUnit::Microseconds,
            None
        )));
        assert!(supports_min_max(&DataType::Duration(
            TimeUnit::Milliseconds
        )));
    }

    #[test]
    fn boolean_is_treated_as_min_max_capable() {
        // Deliberate: booleans reduce to "false"/"true" strings today, and the
        // catalog stores them. Pinned so excluding bool later is a visible diff
        // rather than a silent stat-shape change.
        assert!(supports_min_max(&DataType::Boolean));
    }

    #[test]
    fn min_max_unsupported_for_nested_and_opaque_types() {
        assert!(!supports_min_max(&DataType::List(Box::new(
            DataType::Int64
        ))));
        assert!(!supports_min_max(&DataType::Binary));
    }

    #[test]
    fn null_scalar_formats_as_none() {
        assert_eq!(format_scalar(&Scalar::null(DataType::String)), None);
        assert_eq!(format_scalar(&Scalar::null(DataType::Int64)), None);
    }

    #[test]
    fn string_scalars_format_without_polars_quoting() {
        let borrowed = Scalar::new(DataType::String, AnyValue::String("a"));
        assert_eq!(format_scalar(&borrowed), Some("a".to_string()));

        let owned = Scalar::new(
            DataType::String,
            AnyValue::StringOwned("a".to_string().into()),
        );
        assert_eq!(format_scalar(&owned), Some("a".to_string()));
    }

    #[test]
    fn numeric_scalar_formats_via_display() {
        let scalar = Scalar::new(DataType::Int64, AnyValue::Int64(-7));
        assert_eq!(format_scalar(&scalar), Some("-7".to_string()));
    }

    #[test]
    fn column_stats_cover_numeric_string_and_nullable_columns() {
        let df = polars::df![
            "n" => [3_i64, 1, 2],
            "s" => ["b", "a", "c"],
            "with_nulls" => [Some(5_i64), None, Some(9)],
        ]
        .expect("test frame builds");

        let stats = extract_column_stats(&df, "t1");
        assert_eq!(stats.len(), 3);

        let by_name = |name: &str| {
            stats
                .iter()
                .find(|s| s.column_name == name)
                .expect("column present")
        };

        let n = by_name("n");
        assert_eq!(n.table_id, "t1");
        assert_eq!(n.min_value, Some("1".to_string()));
        assert_eq!(n.max_value, Some("3".to_string()));
        assert_eq!(n.null_count, Some(0));

        let s = by_name("s");
        assert_eq!(s.min_value, Some("a".to_string()));
        assert_eq!(s.max_value, Some("c".to_string()));

        // min/max skip nulls; null_count still reports them.
        let nullable = by_name("with_nulls");
        assert_eq!(nullable.min_value, Some("5".to_string()));
        assert_eq!(nullable.max_value, Some("9".to_string()));
        assert_eq!(nullable.null_count, Some(1));
    }
}

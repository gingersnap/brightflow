//! Scan filters and SQL query builder for file-level pruning.

use std::fmt::Write;

/// Filter applied during catalog-backed scan to prune irrelevant files.
#[derive(Debug, Clone)]
pub enum ScanFilter {
    /// Exact match on a partition key/value
    PartitionEq { key: String, value: String },
    /// Range filter on a partition key
    PartitionRange {
        key: String,
        min: Option<String>,
        max: Option<String>,
    },
    /// Range filter on a column's min/max stats
    ColumnRange {
        column: String,
        min: Option<String>,
        max: Option<String>,
    },
}

/// Build a pruning SQL query + bind values from a table_id and list of filters.
///
/// Returns `(sql_string, bind_values)` ready for `sqlx::query_as`.
pub fn build_pruning_query(table_id: &str, filters: &[ScanFilter]) -> (String, Vec<String>) {
    let mut bind_values: Vec<String> = Vec::new();

    // Collect which partition keys and stat columns we need to join
    let mut partition_filters: Vec<&ScanFilter> = Vec::new();
    let mut column_filters: Vec<&ScanFilter> = Vec::new();

    for f in filters {
        match f {
            ScanFilter::PartitionEq { .. } | ScanFilter::PartitionRange { .. } => {
                partition_filters.push(f);
            },
            ScanFilter::ColumnRange { .. } => {
                column_filters.push(f);
            },
        }
    }

    let mut sql = String::from(
        "SELECT DISTINCT tf.id, tf.table_id, tf.path, tf.num_rows, tf.size_bytes, tf.added_at\nFROM table_files tf\n",
    );

    // Each partition filter needs its own join alias: `file_partitions` holds one
    // row per key, so two filters sharing a single alias would demand one row
    // match both keys at once — unsatisfiable, and the scan silently returns no
    // files. Mirrors the `fcs{i}` aliasing used for column stats below.
    for (i, _) in partition_filters.iter().enumerate() {
        let _ = writeln!(
            sql,
            "INNER JOIN file_partitions fp{i} ON fp{i}.file_id = tf.id"
        );
    }

    // Join file_column_stats for each column filter with a separate alias
    for (i, _) in column_filters.iter().enumerate() {
        let _ = writeln!(
            sql,
            "LEFT JOIN file_column_stats fcs{i} ON fcs{i}.file_id = tf.id"
        );
    }

    sql.push_str("WHERE tf.table_id = ?\n");
    bind_values.push(table_id.to_string());

    // Partition filters
    for (i, f) in partition_filters.iter().enumerate() {
        match f {
            ScanFilter::PartitionEq { key, value } => {
                let _ = writeln!(
                    sql,
                    "  AND fp{i}.partition_key = ? AND fp{i}.partition_value = ?"
                );
                bind_values.push(key.clone());
                bind_values.push(value.clone());
            },
            ScanFilter::PartitionRange { key, min, max } => {
                let _ = writeln!(sql, "  AND fp{i}.partition_key = ?");
                bind_values.push(key.clone());
                if let Some(min_val) = min {
                    let _ = writeln!(sql, "  AND fp{i}.partition_value >= ?");
                    bind_values.push(min_val.clone());
                }
                if let Some(max_val) = max {
                    let _ = writeln!(sql, "  AND fp{i}.partition_value <= ?");
                    bind_values.push(max_val.clone());
                }
            },
            ScanFilter::ColumnRange { .. } => {},
        }
    }

    // Column stat filters: use min/max overlap logic
    // A file's data overlaps the query range [qmin, qmax] if:
    //   file.max_value >= qmin AND file.min_value <= qmax
    // NULLs in stats mean "unknown" → don't prune (keep the file)
    for (i, f) in column_filters.iter().enumerate() {
        if let ScanFilter::ColumnRange { column, min, max } = f {
            let _ = writeln!(sql, "  AND fcs{i}.column_name = ?");
            bind_values.push(column.clone());
            if let Some(min_val) = min {
                let _ = writeln!(
                    sql,
                    "  AND (fcs{i}.max_value IS NULL OR fcs{i}.max_value >= ?)"
                );
                bind_values.push(min_val.clone());
            }
            if let Some(max_val) = max {
                let _ = writeln!(
                    sql,
                    "  AND (fcs{i}.min_value IS NULL OR fcs{i}.min_value <= ?)"
                );
                bind_values.push(max_val.clone());
            }
        }
    }

    // If no column filters, files without stats should still be returned.
    // The LEFT JOIN + no WHERE on fcs handles this.

    (sql, bind_values)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Assertions target structure — bind order, alias presence, placeholder
    // count — rather than exact SQL text, so whitespace refactors don't break
    // them.

    fn partition_eq(key: &str, value: &str) -> ScanFilter {
        ScanFilter::PartitionEq {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn no_filters_binds_only_the_table_id() {
        let (sql, binds) = build_pruning_query("t1", &[]);

        assert_eq!(binds, vec!["t1".to_string()]);
        assert_eq!(sql.matches('?').count(), 1);
        assert!(!sql.contains("INNER JOIN"));
        assert!(!sql.contains("LEFT JOIN"));
    }

    #[test]
    fn partition_eq_binds_table_then_key_then_value() {
        let (sql, binds) = build_pruning_query("t1", &[partition_eq("day", "2026-01-01")]);

        assert_eq!(binds, vec!["t1", "day", "2026-01-01"]);
        assert_eq!(sql.matches('?').count(), 3);
        assert!(sql.contains("INNER JOIN file_partitions fp0 ON fp0.file_id = tf.id"));
        assert!(!sql.contains("fp1."));
    }

    #[test]
    fn two_partition_filters_get_independent_join_aliases() {
        // Regression: both filters once shared a single `fp` alias, demanding one
        // row match two partition keys at once — the query always matched zero
        // files instead of pruning to the intersection.
        let (sql, binds) = build_pruning_query(
            "t1",
            &[
                partition_eq("day", "2026-01-01"),
                partition_eq("region", "eu"),
            ],
        );

        assert_eq!(binds, vec!["t1", "day", "2026-01-01", "region", "eu"]);
        assert!(sql.contains("INNER JOIN file_partitions fp0 ON fp0.file_id = tf.id"));
        assert!(sql.contains("INNER JOIN file_partitions fp1 ON fp1.file_id = tf.id"));
        assert!(sql.contains("AND fp0.partition_key = ? AND fp0.partition_value = ?"));
        assert!(sql.contains("AND fp1.partition_key = ? AND fp1.partition_value = ?"));
    }

    #[test]
    fn partition_range_without_bounds_binds_only_the_key() {
        let (sql, binds) = build_pruning_query(
            "t1",
            &[ScanFilter::PartitionRange {
                key: "day".to_string(),
                min: None,
                max: None,
            }],
        );

        assert_eq!(binds, vec!["t1", "day"]);
        assert_eq!(sql.matches('?').count(), 2);
        assert!(!sql.contains("partition_value"));
    }

    #[test]
    fn partition_range_binds_min_before_max() {
        let (_sql, binds) = build_pruning_query(
            "t1",
            &[ScanFilter::PartitionRange {
                key: "day".to_string(),
                min: Some("2026-01-01".to_string()),
                max: Some("2026-02-01".to_string()),
            }],
        );

        assert_eq!(binds, vec!["t1", "day", "2026-01-01", "2026-02-01"]);
    }

    #[test]
    fn column_range_without_bounds_binds_only_the_column() {
        let (sql, binds) = build_pruning_query(
            "t1",
            &[ScanFilter::ColumnRange {
                column: "amount".to_string(),
                min: None,
                max: None,
            }],
        );

        assert_eq!(binds, vec!["t1", "amount"]);
        assert_eq!(sql.matches('?').count(), 2);
        assert!(sql.contains("LEFT JOIN file_column_stats fcs0 ON fcs0.file_id = tf.id"));
    }

    #[test]
    fn column_range_binds_min_before_max() {
        let (sql, binds) = build_pruning_query(
            "t1",
            &[ScanFilter::ColumnRange {
                column: "amount".to_string(),
                min: Some("10".to_string()),
                max: Some("99".to_string()),
            }],
        );

        assert_eq!(binds, vec!["t1", "amount", "10", "99"]);
        assert!(sql.contains("fcs0.max_value IS NULL OR fcs0.max_value >= ?"));
        assert!(sql.contains("fcs0.min_value IS NULL OR fcs0.min_value <= ?"));
    }

    #[test]
    fn mixed_filters_bind_table_then_partitions_then_columns() {
        // Bind order must follow the emitted predicate order regardless of how
        // the caller interleaves filter kinds.
        let (sql, binds) = build_pruning_query(
            "t1",
            &[
                ScanFilter::ColumnRange {
                    column: "amount".to_string(),
                    min: Some("10".to_string()),
                    max: None,
                },
                partition_eq("day", "2026-01-01"),
            ],
        );

        assert_eq!(binds, vec!["t1", "day", "2026-01-01", "amount", "10"]);
        assert_eq!(sql.matches('?').count(), 5);
    }
}

//! SQLite row types for Litehouse metadata

use serde::{Deserialize, Serialize};

/// A table row from the `tables` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TableRow {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub schema_json: Option<String>,
    pub primary_keys: Option<String>,
    pub total_rows: i64,
    pub created_at: String,
    pub updated_at: String,
    pub partition_columns: Option<String>,
}

/// A file entry row from the `table_files` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TableFileRow {
    pub id: String,
    pub table_id: String,
    pub path: String,
    pub num_rows: i64,
    pub size_bytes: i64,
    pub added_at: String,
}

/// Column-level statistics from the `table_column_stats` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ColumnStatRow {
    pub table_id: String,
    pub column_name: String,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub null_count: Option<i64>,
}

/// A partition key/value pair for a file
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FilePartitionRow {
    pub file_id: String,
    pub partition_key: String,
    pub partition_value: String,
}

/// Per-file column statistics for file-level pruning
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FileColumnStatRow {
    pub file_id: String,
    pub column_name: String,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub null_count: Option<i64>,
}

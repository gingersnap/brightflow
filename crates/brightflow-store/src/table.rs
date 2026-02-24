//! Table operations for Delta Lake

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use deltalake::{open_table as delta_open_table, DeltaTable};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;
use url::Url;

use crate::error::{StoreError, StoreResult};

/// Reference to a table in the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRef {
    /// Table name
    pub name: String,
    /// Path to the table
    pub path: String,
}

/// Information about a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    /// Table name
    pub name: String,
    /// Path to the table
    pub path: String,
    /// Current version
    pub version: i64,
    /// Number of rows (approximate, from metadata)
    pub num_rows: Option<i64>,
    /// Number of files
    pub num_files: usize,
    /// Schema as JSON
    pub schema: Option<serde_json::Value>,
    /// Created timestamp
    pub created_at: Option<DateTime<Utc>>,
    /// Last modified timestamp
    pub updated_at: Option<DateTime<Utc>>,
}

/// Convert a path to a URL for Delta Lake
pub fn path_to_url(path: &Path) -> StoreResult<Url> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    Url::from_file_path(&abs_path)
        .map_err(|()| StoreError::Other(format!("Invalid path: {}", abs_path.display())))
}

/// Check if a directory is a Delta table
pub async fn table_exists(path: &Path) -> StoreResult<bool> {
    let delta_log = path.join("_delta_log");
    Ok(delta_log.exists() && delta_log.is_dir())
}

/// List all tables in a directory
pub async fn list_tables(root: &Path) -> StoreResult<Vec<TableRef>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut tables = Vec::new();
    let entries = std::fs::read_dir(root)?;

    for entry_result in entries {
        let entry = entry_result?;
        let path = entry.path();

        if path.is_dir() {
            let delta_log = path.join("_delta_log");
            if delta_log.exists() && delta_log.is_dir() {
                let name = entry.file_name().to_str().unwrap_or_default().to_string();
                tables.push(TableRef {
                    name,
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }

    Ok(tables)
}

/// Open an existing Delta table
pub async fn open_table(path: &Path) -> StoreResult<DeltaTable> {
    let url = path_to_url(path)?;
    debug!("Opening Delta table at {}", url);

    let table = delta_open_table(url).await.map_err(|e| {
        let err_str = e.to_string();
        if err_str.contains("not found") || err_str.contains("does not exist") {
            StoreError::TableNotFound(path.to_string_lossy().to_string())
        } else {
            StoreError::DeltaLake(e)
        }
    })?;

    Ok(table)
}

/// Get information about a table
pub async fn get_table_info(name: &str, path: &Path) -> StoreResult<TableInfo> {
    let table = open_table(path).await?;
    let snapshot = table
        .snapshot()
        .map_err(|e| StoreError::Other(e.to_string()))?;
    let metadata = snapshot.metadata();

    // Get schema as JSON - schema() returns Arc<StructType>
    let schema = snapshot.schema();
    let schema_json = serde_json::to_value(&*schema).ok();

    // Count parquet files in the table directory
    let num_files = get_parquet_paths(path)?.len();

    // Get row count from Delta log metadata
    let num_rows = get_row_count_from_log(path);

    // Get created timestamp
    let created_at = metadata
        .created_time()
        .and_then(DateTime::from_timestamp_millis);

    // Get version
    let version = table.version().unwrap_or(0);

    Ok(TableInfo {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        version,
        num_rows,
        num_files,
        schema: schema_json,
        created_at,
        updated_at: None,
    })
}

/// Extract row count from Delta log files by parsing the stats JSON
fn get_row_count_from_log(path: &Path) -> Option<i64> {
    let log_path = path.join("_delta_log");
    if !log_path.exists() {
        return None;
    }

    let mut total_rows: i64 = 0;
    let mut found_any = false;

    // Read all JSON log files and sum numRecords from add actions
    if let Ok(entries) = std::fs::read_dir(&log_path) {
        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    for line in content.lines() {
                        if let Some(count) = extract_num_records_from_action(line) {
                            total_rows += count;
                            found_any = true;
                        }
                    }
                }
            }
        }
    }

    if found_any {
        Some(total_rows)
    } else {
        None
    }
}

/// Extract numRecords from a Delta log action line
fn extract_num_records_from_action(line: &str) -> Option<i64> {
    // Parse the line as JSON
    let value: serde_json::Value = serde_json::from_str(line).ok()?;

    // Check if this is an "add" action with stats
    let add = value.get("add")?;
    let stats_str = add.get("stats")?.as_str()?;

    // Parse the stats JSON string
    let stats: serde_json::Value = serde_json::from_str(stats_str).ok()?;
    stats.get("numRecords")?.as_i64()
}

/// Get paths to all parquet files in a directory
pub fn get_parquet_paths(path: &Path) -> StoreResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if path.exists() && path.is_dir() {
        for dir_entry in std::fs::read_dir(path)? {
            let file_path = dir_entry?.path();
            if file_path.extension().is_some_and(|ext| ext == "parquet") {
                paths.push(file_path);
            }
        }
    }
    Ok(paths)
}

/// Read a Delta table as a Polars DataFrame
pub async fn read_table(path: &Path) -> StoreResult<DataFrame> {
    let table = open_table(path).await?;
    read_delta_table_to_df(&table).await
}

/// Read a specific version of a Delta table as a Polars DataFrame
pub async fn read_table_version(path: &Path, version: i64) -> StoreResult<DataFrame> {
    let url = path_to_url(path)?;
    let table = deltalake::open_table_with_version(url, version).await?;
    read_delta_table_to_df(&table).await
}

/// Convert a Delta table to a Polars DataFrame using Polars' eager ParquetReader.
/// Uses spawn_blocking to avoid blocking the async runtime.
async fn read_delta_table_to_df(table: &DeltaTable) -> StoreResult<DataFrame> {
    let base_url = table.table_url();
    let base_path = base_url
        .to_file_path()
        .map_err(|()| StoreError::Other(format!("Cannot convert URL to file path: {base_url}")))?;

    let parquet_files = get_parquet_paths(&base_path)?;

    if parquet_files.is_empty() {
        return Ok(DataFrame::empty());
    }

    debug!(
        "Reading {} parquet file(s) from {}",
        parquet_files.len(),
        base_path.display()
    );

    let df = tokio::task::spawn_blocking(move || -> StoreResult<DataFrame> {
        let mut combined: Option<DataFrame> = None;
        for file_path in &parquet_files {
            debug!("Reading parquet file: {}", file_path.display());
            let file = std::fs::File::open(file_path)?;
            let df = ParquetReader::new(file).finish()?;
            combined = Some(match combined {
                Some(mut existing) => {
                    existing.vstack_mut(&df)?;
                    existing
                },
                None => df,
            });
        }
        Ok(combined.unwrap_or_else(DataFrame::empty))
    })
    .await
    .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??;

    Ok(df)
}

/// Delete a Delta table
pub async fn delete_table(path: &Path) -> StoreResult<()> {
    if !table_exists(path).await? {
        return Err(StoreError::TableNotFound(
            path.to_string_lossy().to_string(),
        ));
    }

    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_table_not_exists() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let result = table_exists(&tmp.path().join("nonexistent")).await;
        assert!(result.is_ok());
        assert!(!result.expect("table_exists failed"));
    }
}

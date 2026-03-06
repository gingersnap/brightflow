//! Table operations for manifest-based Parquet store

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::{StoreError, StoreResult};
use crate::manifest::{migrate_from_delta_if_needed, Manifest};

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
    /// Number of rows (from manifest)
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

/// Check if a directory is a manifest-based table
pub async fn table_exists(path: &Path) -> StoreResult<bool> {
    // Auto-migrate Delta tables if found
    migrate_from_delta_if_needed(path)?;
    Ok(Manifest::exists(path))
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
            // Auto-migrate Delta tables if found
            migrate_from_delta_if_needed(&path)?;

            if Manifest::exists(&path) {
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

/// Get information about a table
pub async fn get_table_info(name: &str, path: &Path) -> StoreResult<TableInfo> {
    migrate_from_delta_if_needed(path)?;

    let manifest = Manifest::load(path)?;

    Ok(TableInfo {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        version: i64::try_from(manifest.version).unwrap_or(0),
        num_rows: Some(manifest.total_rows),
        num_files: manifest.files.len(),
        schema: manifest.schema,
        created_at: Some(manifest.created_at),
        updated_at: Some(manifest.updated_at),
    })
}

/// Get paths to all parquet files listed in the manifest
pub fn get_parquet_paths(path: &Path) -> StoreResult<Vec<PathBuf>> {
    let manifest = Manifest::load(path)?;
    let paths = manifest.files.iter().map(|f| path.join(&f.path)).collect();
    Ok(paths)
}

/// Read a table as a Polars DataFrame
pub async fn read_table(path: &Path) -> StoreResult<DataFrame> {
    migrate_from_delta_if_needed(path)?;

    let manifest = Manifest::load(path)?;

    if manifest.files.is_empty() {
        return Ok(DataFrame::empty());
    }

    let base_path = path.to_path_buf();
    let file_paths: Vec<PathBuf> = manifest
        .files
        .iter()
        .map(|f| base_path.join(&f.path))
        .collect();

    debug!(
        "Reading {} parquet file(s) from {}",
        file_paths.len(),
        base_path.display()
    );

    let df = tokio::task::spawn_blocking(move || -> StoreResult<DataFrame> {
        let mut combined: Option<DataFrame> = None;
        for file_path in &file_paths {
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

/// Delete a table
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

//! Table operations for SQLite-backed Parquet store

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;
use ts_rs::TS;

use crate::db::StoreDb;
use crate::error::{StoreError, StoreResult};
use crate::ingest::concat_df;

/// Reference to a table in the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRef {
    /// Table name
    pub name: String,
    /// Owning source (e.g. `web:<uuid>` or `connector:<uuid>`)
    pub source_id: String,
    /// Path to the table
    pub path: String,
}

/// Column-level statistics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ColumnStat {
    pub column_name: String,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    #[ts(type = "number | null")]
    pub null_count: Option<i64>,
}

/// Information about a table
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TableInfo {
    /// Table name
    pub name: String,
    /// Owning source (e.g. `web:<uuid>` or `connector:<uuid>`)
    pub source_id: String,
    /// Path to the table
    pub path: String,
    /// Current version
    #[ts(type = "number")]
    pub version: i64,
    /// Number of rows
    #[ts(type = "number | null")]
    pub num_rows: Option<i64>,
    /// Number of files
    pub num_files: usize,
    /// Schema as JSON
    #[ts(type = "unknown")]
    pub schema: Option<serde_json::Value>,
    /// Column-level statistics (min/max/null_count)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_stats: Vec<ColumnStat>,
    /// Created timestamp
    pub created_at: Option<DateTime<Utc>>,
    /// Last modified timestamp
    pub updated_at: Option<DateTime<Utc>>,
}

/// Check if a table exists in the database for the given source
pub async fn table_exists(db: &StoreDb, source_id: &str, name: &str) -> StoreResult<bool> {
    Ok(db.get_table(source_id, name).await?.is_some())
}

/// List all tables from the database
pub async fn list_tables(db: &StoreDb, root: &Path) -> StoreResult<Vec<TableRef>> {
    let rows = db.list_tables().await?;
    Ok(rows
        .into_iter()
        .map(|row| TableRef {
            path: root
                .join(&row.source_id)
                .join(&row.name)
                .to_string_lossy()
                .to_string(),
            source_id: row.source_id,
            name: row.name,
        })
        .collect())
}

/// Get information about a table
pub async fn get_table_info(
    db: &StoreDb,
    source_id: &str,
    name: &str,
    root: &Path,
) -> StoreResult<TableInfo> {
    let row = db
        .get_table(source_id, name)
        .await?
        .ok_or_else(|| StoreError::TableNotFound(name.to_string()))?;

    let files = db.list_table_files(&row.id).await?;
    let stat_rows = db.get_column_stats(&row.id).await?;

    let schema: Option<serde_json::Value> = row
        .schema_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    let created_at = row.created_at.parse::<DateTime<Utc>>().ok();
    let updated_at = row.updated_at.parse::<DateTime<Utc>>().ok();

    let column_stats = stat_rows
        .into_iter()
        .map(|s| ColumnStat {
            column_name: s.column_name,
            min_value: s.min_value,
            max_value: s.max_value,
            null_count: s.null_count,
        })
        .collect();

    let path = root
        .join(&row.source_id)
        .join(&row.name)
        .to_string_lossy()
        .to_string();
    Ok(TableInfo {
        name: row.name.clone(),
        source_id: row.source_id,
        path,
        version: row.version,
        num_rows: Some(row.total_rows),
        num_files: files.len(),
        schema,
        column_stats,
        created_at,
        updated_at,
    })
}

/// Resolve a stored file path: absolute paths pass through, relative paths
/// are resolved against `root`.
fn resolve_path(root: &Path, stored_path: &str) -> PathBuf {
    let p = Path::new(stored_path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Get paths to all parquet files for a table
pub async fn get_parquet_paths(
    db: &StoreDb,
    source_id: &str,
    name: &str,
    root: &Path,
) -> StoreResult<Vec<PathBuf>> {
    let row = db
        .get_table(source_id, name)
        .await?
        .ok_or_else(|| StoreError::TableNotFound(name.to_string()))?;

    let files = db.list_table_files(&row.id).await?;
    Ok(files.iter().map(|f| resolve_path(root, &f.path)).collect())
}

/// Read a table as a Polars DataFrame
pub async fn read_table(
    db: &StoreDb,
    source_id: &str,
    name: &str,
    root: &Path,
) -> StoreResult<DataFrame> {
    let file_paths = get_parquet_paths(db, source_id, name, root).await?;

    if file_paths.is_empty() {
        return Ok(DataFrame::empty());
    }

    debug!(
        "Reading {} parquet file(s) for table '{}'",
        file_paths.len(),
        name
    );

    let df = tokio::task::spawn_blocking(move || -> StoreResult<DataFrame> {
        let mut frames = Vec::new();
        for file_path in &file_paths {
            debug!("Reading parquet file: {}", file_path.display());
            let file = std::fs::File::open(file_path)?;
            let df = ParquetReader::new(file).finish()?;
            frames.push(df);
        }
        concat_df(&frames)
    })
    .await
    .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??;

    Ok(df)
}

/// Delete a table (db record + files on disk)
pub async fn delete_table(
    db: &StoreDb,
    source_id: &str,
    name: &str,
    root: &Path,
) -> StoreResult<()> {
    let existed = db.delete_table(source_id, name).await?;
    if !existed {
        return Err(StoreError::TableNotFound(name.to_string()));
    }

    let table_dir = root.join(source_id).join(name);
    if table_dir.exists() {
        std::fs::remove_dir_all(&table_dir)?;
    }

    Ok(())
}

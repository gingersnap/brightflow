//! Read-side table operations: listing, metadata, and whole-table reads.
//!
//! A table read concatenates every registered Parquet file rather than
//! trusting cached metadata — the files are the data of record and the
//! catalog rows are an index over them.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;
use ts_rs::TS;

use brightflow_types::TableSchema;

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
    /// The table's columns and logical types.
    #[ts(optional)]
    pub schema: Option<TableSchema>,
    /// Column-level statistics (min/max/null_count)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_stats: Vec<ColumnStat>,
    /// Created timestamp
    pub created_at: Option<DateTime<Utc>>,
    /// Last modified timestamp
    pub updated_at: Option<DateTime<Utc>>,
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

    let schema = row.schema();

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

/// The inverse of [`resolve_path`]: express `path` relative to `root` so the
/// stored form survives the workspace being moved or copied.
///
/// A stored absolute path pins a table's files to the machine that wrote them —
/// copy the workspace and every file in it is gone. Event partitions live
/// beside the store rather than inside it (`{workspace}/events/` next to
/// `{workspace}/store/`), so the answer legitimately walks up with `..`; that
/// is still root-relative and still moves with the workspace.
///
/// Returns `None` when the two share no common ancestor (different drives, or
/// a genuinely unrelated location), in which case the caller must keep the
/// absolute path — a wrong relative path would be worse than an unportable one.
pub fn relativize_path(root: &Path, path: &Path) -> Option<PathBuf> {
    use std::path::Component;

    // Only absolute, already-normalized inputs can be compared component-wise.
    if !root.is_absolute() || !path.is_absolute() {
        return None;
    }

    let root_parts: Vec<Component<'_>> = root.components().collect();
    let path_parts: Vec<Component<'_>> = path.components().collect();
    let shared = root_parts
        .iter()
        .zip(&path_parts)
        .take_while(|(a, b)| a == b)
        .count();

    // No shared prefix at all means no meaningful relative form. On Unix every
    // absolute path shares RootDir, so this only trips across Windows drives.
    if shared == 0 {
        return None;
    }

    let mut out = PathBuf::new();
    for _ in shared..root_parts.len() {
        out.push("..");
    }
    // `shared <= path_parts.len()` by construction (it counts zipped pairs),
    // but take the checked form so the invariant cannot rot into a panic.
    for part in path_parts.get(shared..).unwrap_or(&[]) {
        out.push(part);
    }
    // Identical paths produce an empty result, which is not a usable file path.
    if out.as_os_str().is_empty() {
        return None;
    }
    Some(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relativize_path_round_trips_through_resolve() {
        let root = Path::new("/data/ws/store");
        // Inside the store root.
        let inside = Path::new("/data/ws/store/src1/issues/f.parquet");
        let rel = relativize_path(root, inside).expect("relative form");
        assert_eq!(rel, PathBuf::from("src1/issues/f.parquet"));
        assert_eq!(resolve_path(root, &rel.to_string_lossy()), inside);

        // Beside the store root: event partitions legitimately walk up.
        let beside = Path::new("/data/ws/events/src1/2026-08-29/f.parquet");
        let rel = relativize_path(root, beside).expect("relative form");
        assert_eq!(rel, PathBuf::from("../events/src1/2026-08-29/f.parquet"));
        // resolve_path joins without normalizing; the filesystem resolves `..`.
        assert_eq!(
            resolve_path(root, &rel.to_string_lossy()),
            PathBuf::from("/data/ws/store/../events/src1/2026-08-29/f.parquet")
        );
    }

    #[test]
    fn relativize_path_refuses_what_it_cannot_express() {
        let root = Path::new("/data/ws/store");
        // Relative inputs cannot be compared component-wise.
        assert!(relativize_path(root, Path::new("relative/f.parquet")).is_none());
        assert!(relativize_path(Path::new("relative"), Path::new("/a/f.parquet")).is_none());
        // Root itself has no file to point at.
        assert!(relativize_path(root, root).is_none());
    }

    #[test]
    fn resolve_path_joins_relative_paths_onto_root() {
        let out = resolve_path(Path::new("/data/store"), "src1/issues/f.parquet");
        assert_eq!(out, PathBuf::from("/data/store/src1/issues/f.parquet"));
    }

    #[test]
    fn resolve_path_passes_absolute_paths_through() {
        let out = resolve_path(Path::new("/data/store"), "/elsewhere/f.parquet");
        assert_eq!(out, PathBuf::from("/elsewhere/f.parquet"));
    }
}

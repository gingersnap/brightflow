//! Data ingestion into SQLite-backed Parquet tables

use std::path::Path;

use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::db::StoreDb;
use crate::error::{StoreError, StoreResult};
use crate::stats;

/// Metrics from a merge (upsert) operation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MergeMetrics {
    pub rows_updated: usize,
    pub rows_inserted: usize,
}

/// Options for data ingestion
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IngestOptions {
    /// Save mode: Append, Overwrite, ErrorIfExists, Ignore
    #[serde(default)]
    pub mode: IngestMode,

    /// Partition columns (reserved for future use)
    #[serde(default)]
    pub partition_by: Vec<String>,

    /// Table description
    pub description: Option<String>,
}

/// Mode for ingestion
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IngestMode {
    /// Append to existing data
    #[default]
    Append,
    /// Overwrite existing data
    Overwrite,
    /// Error if table exists
    ErrorIfExists,
    /// Ignore if table exists
    Ignore,
}

/// Generate a new Parquet filename using UUIDv7, scoped by source_id and table name.
fn new_parquet_path(source_id: &str, table_name: &str) -> String {
    let id = uuid::Uuid::now_v7();
    format!("{source_id}/{table_name}/{id}.parquet")
}

/// Ingest a Parquet file into a table
pub async fn ingest_parquet(
    db: &StoreDb,
    root: &Path,
    source_id: &str,
    table_name: &str,
    parquet_path: &Path,
    options: &IngestOptions,
) -> StoreResult<()> {
    if !parquet_path.exists() {
        return Err(StoreError::FileNotFound(parquet_path.to_path_buf()));
    }

    let existing = db.get_table(source_id, table_name).await?;

    if let Some(ref row) = existing {
        match options.mode {
            IngestMode::ErrorIfExists => {
                return Err(StoreError::TableAlreadyExists(table_name.to_string()));
            },
            IngestMode::Ignore => {
                debug!("Table exists and mode is Ignore, skipping");
                return Ok(());
            },
            IngestMode::Overwrite => {
                // Delete old files from disk
                let files = db.list_table_files(&row.id).await?;
                for file_row in &files {
                    let old_path = root.join(&file_row.path);
                    if old_path.exists() {
                        std::fs::remove_file(&old_path)?;
                    }
                }
                db.delete_table_files(&row.id).await?;
            },
            IngestMode::Append => {},
        }
    }

    // Ensure source/table directory exists
    let table_dir = root.join(source_id).join(table_name);
    std::fs::create_dir_all(&table_dir)?;

    // Generate UUIDv7 filename and copy file
    let relative_path = new_parquet_path(source_id, table_name);
    let dest_path = root.join(&relative_path);
    std::fs::copy(parquet_path, &dest_path)?;

    // Read metadata from the copied file
    let file = std::fs::File::open(&dest_path)?;
    let df = ParquetReader::new(file).finish()?;
    let num_rows = i64::try_from(df.height()).unwrap_or(0);

    if num_rows == 0 {
        debug!("Parquet file is empty, skipping ingestion");
        std::fs::remove_file(&dest_path)?;
        return Ok(());
    }

    let schema_json = schema_to_json(df.schema().as_ref());
    let schema_str =
        serde_json::to_string(&schema_json).map_err(|e| StoreError::Other(e.to_string()))?;
    let file_size = i64::try_from(std::fs::metadata(&dest_path)?.len()).unwrap_or(0);

    // Create or update table record
    let table_row = if let Some(row) = existing {
        if options.mode == IngestMode::Overwrite {
            db.update_table_meta(&row.id, Some(&schema_str), None, num_rows)
                .await?
                .ok_or_else(|| StoreError::Other("Failed to update table".into()))?
        } else {
            let new_total = row.total_rows + num_rows;
            db.update_table_meta(&row.id, Some(&schema_str), None, new_total)
                .await?
                .ok_or_else(|| StoreError::Other("Failed to update table".into()))?
        }
    } else {
        let row = db.create_table(table_name, source_id).await?;
        db.update_table_meta(&row.id, Some(&schema_str), None, num_rows)
            .await?
            .ok_or_else(|| StoreError::Other("Failed to update table".into()))?
    };

    // Register the file
    db.add_table_file(&table_row.id, &relative_path, num_rows, file_size)
        .await?;

    // Extract and store column-level statistics
    let column_stats = stats::extract_column_stats(&df, &table_row.id);
    db.upsert_column_stats(&table_row.id, &column_stats).await?;

    Ok(())
}

/// Merge (upsert) a Parquet file into a table by primary keys.
/// Uses Polars anti_join/semi_join for the merge logic.
pub async fn merge_parquet(
    db: &StoreDb,
    root: &Path,
    source_id: &str,
    table_name: &str,
    parquet_path: &Path,
    primary_keys: &[String],
) -> StoreResult<MergeMetrics> {
    if primary_keys.is_empty() {
        return Err(StoreError::Other(
            "merge_parquet requires at least one primary key".into(),
        ));
    }

    if !parquet_path.exists() {
        return Err(StoreError::FileNotFound(parquet_path.to_path_buf()));
    }

    // Get or create table record
    let table_row = if let Some(row) = db.get_table(source_id, table_name).await? {
        row
    } else {
        let row = db.create_table(table_name, source_id).await?;
        let pks_json =
            serde_json::to_string(primary_keys).map_err(|e| StoreError::Other(e.to_string()))?;
        db.update_table_meta(&row.id, None, Some(&pks_json), 0)
            .await?
            .ok_or_else(|| StoreError::Other("Failed to update table".into()))?
    };

    let table_id = table_row.id.clone();

    // Gather existing file paths from db (before spawn_blocking)
    let existing_files = db.list_table_files(&table_id).await?;
    let existing_file_paths: Vec<std::path::PathBuf> =
        existing_files.iter().map(|f| root.join(&f.path)).collect();
    let old_relative_paths: Vec<String> = existing_files.iter().map(|f| f.path.clone()).collect();

    let source_path = parquet_path.to_path_buf();
    let pks: Vec<String> = primary_keys.to_vec();

    // CPU-heavy Polars work in spawn_blocking
    let (result_df, metrics) =
        tokio::task::spawn_blocking(move || -> StoreResult<(DataFrame, MergeMetrics)> {
            let source_file = std::fs::File::open(&source_path)?;
            let source = ParquetReader::new(source_file).finish()?;

            if source.is_empty() {
                debug!("Parquet file is empty, skipping merge");
                return Ok((DataFrame::empty(), MergeMetrics::default()));
            }

            let source_height = source.height();

            if existing_file_paths.is_empty() {
                // No existing data — just write source
                return Ok((
                    source,
                    MergeMetrics {
                        rows_updated: 0,
                        rows_inserted: source_height,
                    },
                ));
            }

            // Read existing data
            let mut frames = Vec::new();
            for file_path in &existing_file_paths {
                let file = std::fs::File::open(file_path)?;
                let df = ParquetReader::new(file).finish()?;
                frames.push(df);
            }
            let existing = concat_df(&frames)?;

            if existing.is_empty() {
                return Ok((
                    source,
                    MergeMetrics {
                        rows_updated: 0,
                        rows_inserted: source_height,
                    },
                ));
            }

            // Polars upsert via join:
            // matched = existing rows that have matching PKs in source
            let matched_count = existing
                .join(
                    &source,
                    pks.as_slice(),
                    pks.as_slice(),
                    JoinArgs::new(JoinType::Semi),
                    None,
                )?
                .height();

            // unchanged = existing rows NOT in source
            let unchanged = existing.join(
                &source,
                pks.as_slice(),
                pks.as_slice(),
                JoinArgs::new(JoinType::Anti),
                None,
            )?;

            // result = source rows (new + updated) + unchanged
            let mut result = concat_df(&[source, unchanged])?;
            result.rechunk_mut();

            let rows_inserted = source_height.saturating_sub(matched_count);

            info!(
                "Merge complete: {} updated, {} inserted",
                matched_count, rows_inserted
            );

            Ok((
                result,
                MergeMetrics {
                    rows_updated: matched_count,
                    rows_inserted,
                },
            ))
        })
        .await
        .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??;

    // If the merge produced nothing, return early
    if result_df.is_empty() && metrics.rows_inserted == 0 && metrics.rows_updated == 0 {
        return Ok(metrics);
    }

    // Ensure source/table directory exists
    let table_dir = root.join(source_id).join(table_name);
    std::fs::create_dir_all(&table_dir)?;

    // Write merged result to new UUIDv7 file
    let relative_path = new_parquet_path(source_id, table_name);
    let dest_path = root.join(&relative_path);
    write_parquet(&result_df, &dest_path)?;

    let file_size = i64::try_from(std::fs::metadata(&dest_path)?.len()).unwrap_or(0);
    let num_rows = i64::try_from(result_df.height()).unwrap_or(0);

    // Update db: replace files in a transaction
    let schema_json = schema_to_json(result_df.schema().as_ref());
    let schema_str =
        serde_json::to_string(&schema_json).map_err(|e| StoreError::Other(e.to_string()))?;
    let pks_json =
        serde_json::to_string(primary_keys).map_err(|e| StoreError::Other(e.to_string()))?;

    db.replace_table_files(&table_id, &[(relative_path, num_rows, file_size)])
        .await?;
    db.update_table_meta(&table_id, Some(&schema_str), Some(&pks_json), num_rows)
        .await?;

    // Delete old parquet files from disk
    for old_path in &old_relative_paths {
        let full_path = root.join(old_path);
        if full_path.exists() {
            std::fs::remove_file(&full_path)?;
        }
    }

    // Extract and store column-level statistics
    let column_stats = stats::extract_column_stats(&result_df, &table_id);
    db.upsert_column_stats(&table_id, &column_stats).await?;

    Ok(metrics)
}

/// Replace a table's entire contents with `df` in one consolidated file.
///
/// Mirrors `merge_parquet`'s replace flow: write one uuidv7 parquet, swap the
/// file registry transactionally, update table meta + stats, then delete the
/// old files from disk. When `expected_version` is given, the swap is refused
/// with `StoreError::VersionConflict` if the table's version moved (a sync
/// merged concurrently) — the caller re-reads and retries.
pub async fn replace_table_data(
    db: &StoreDb,
    root: &Path,
    source_id: &str,
    table_name: &str,
    df: DataFrame,
    expected_version: Option<i64>,
) -> StoreResult<()> {
    let table_row = db
        .get_table(source_id, table_name)
        .await?
        .ok_or_else(|| StoreError::TableNotFound(table_name.to_string()))?;

    if let Some(expected) = expected_version {
        if table_row.version != expected {
            return Err(StoreError::VersionConflict {
                table: table_name.to_string(),
                expected,
                found: table_row.version,
            });
        }
    }

    let existing_files = db.list_table_files(&table_row.id).await?;
    let old_paths: Vec<std::path::PathBuf> =
        existing_files.iter().map(|f| root.join(&f.path)).collect();

    let table_dir = root.join(source_id).join(table_name);
    std::fs::create_dir_all(&table_dir)?;

    let relative_path = new_parquet_path(source_id, table_name);
    let dest_path = root.join(&relative_path);

    let write_df = df.clone();
    let write_path = dest_path.clone();
    tokio::task::spawn_blocking(move || -> StoreResult<()> {
        let mut rechunked = write_df;
        rechunked.rechunk_mut();
        let file = std::fs::File::create(&write_path)?;
        ParquetWriter::new(file)
            .with_compression(ParquetCompression::Zstd(None))
            .finish(&mut rechunked)?;
        Ok(())
    })
    .await
    .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??;

    let file_size = i64::try_from(std::fs::metadata(&dest_path)?.len()).unwrap_or(0);
    let num_rows = i64::try_from(df.height()).unwrap_or(0);
    let schema_json = schema_to_json(df.schema().as_ref());
    let schema_str =
        serde_json::to_string(&schema_json).map_err(|e| StoreError::Other(e.to_string()))?;

    db.replace_table_files(&table_row.id, &[(relative_path, num_rows, file_size)])
        .await?;
    db.update_table_meta(&table_row.id, Some(&schema_str), None, num_rows)
        .await?;

    let column_stats = stats::extract_column_stats(&df, &table_row.id);
    db.upsert_column_stats(&table_row.id, &column_stats).await?;

    for old_path in &old_paths {
        // The new file has a fresh uuid name, so it can never be in this list.
        if old_path.exists() {
            std::fs::remove_file(old_path)?;
        }
    }

    info!(
        "Replaced table '{}' data for source '{}' ({} rows, 1 file)",
        table_name, source_id, num_rows
    );
    Ok(())
}

/// Write a DataFrame to a parquet file
fn write_parquet(df: &DataFrame, path: &Path) -> StoreResult<()> {
    let file = std::fs::File::create(path)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;
    Ok(())
}

/// Convert a Polars Schema to a JSON value for storage
pub(crate) fn schema_to_json(schema: &Schema) -> serde_json::Value {
    let fields: Vec<serde_json::Value> = schema
        .iter_fields()
        .map(|field| {
            serde_json::json!({
                "name": field.name.as_str(),
                "type": format!("{}", field.dtype),
                "nullable": true
            })
        })
        .collect();
    serde_json::json!({ "fields": fields })
}

/// Helper to concatenate DataFrames with schema alignment.
///
/// Computes the union of all column names across DataFrames, fills missing
/// columns with nulls (using the dtype from whichever DataFrame has the column),
/// and reorders columns consistently before vstacking.
pub(crate) fn concat_df(dfs: &[DataFrame]) -> StoreResult<DataFrame> {
    if dfs.is_empty() {
        return Ok(DataFrame::empty());
    }
    if dfs.len() == 1 {
        return Ok(dfs[0].clone());
    }

    // Collect the union of all column names, preserving insertion order
    let mut all_columns: Vec<PlSmallStr> = Vec::new();
    let mut seen = PlHashSet::new();
    for df in dfs {
        for name in df.get_column_names() {
            if seen.insert(name.clone()) {
                all_columns.push(name.clone());
            }
        }
    }

    // Build a dtype map: first DataFrame that has the column wins
    let mut dtype_map = PlHashMap::new();
    for col_name in &all_columns {
        for df in dfs {
            if let Ok(col) = df.column(col_name.as_str()) {
                dtype_map.insert(col_name.clone(), col.dtype().clone());
                break;
            }
        }
    }

    // Align each DataFrame to the union schema
    let aligned: Vec<DataFrame> = dfs
        .iter()
        .map(|df| -> PolarsResult<DataFrame> {
            let columns: Vec<Column> = all_columns
                .iter()
                .map(|col_name| {
                    if let Ok(col) = df.column(col_name.as_str()) {
                        col.clone()
                    } else {
                        let dtype = dtype_map.get(col_name).cloned().unwrap_or(DataType::Null);
                        Column::new_scalar(
                            col_name.clone(),
                            Scalar::new(dtype, AnyValue::Null),
                            df.height(),
                        )
                    }
                })
                .collect();
            DataFrame::new(columns)
        })
        .collect::<PolarsResult<Vec<_>>>()?;

    let mut result = aligned[0].clone();
    for df in &aligned[1..] {
        result.vstack_mut(df)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParquetStore;
    use tempfile::TempDir;

    #[test]
    fn test_ingest_mode_default() {
        let mode = IngestMode::default();
        assert_eq!(mode, IngestMode::Append);
    }

    #[test]
    fn test_ingest_options_default() {
        let opts = IngestOptions::default();
        assert_eq!(opts.mode, IngestMode::Append);
        assert!(opts.partition_by.is_empty());
        assert!(opts.description.is_none());
    }

    /// Writing the same table name under two different source_ids must produce
    /// two independent catalog rows — no cross-source data pollution.
    #[tokio::test]
    async fn test_same_table_name_isolated_per_source() {
        let tmp = TempDir::new().expect("tempdir");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        let store = ParquetStore::new(tmp.path(), &db_url).await.expect("store");

        // Build a small parquet file to ingest twice.
        let mut df = df!("id" => [1i64, 2, 3], "value" => ["a", "b", "c"]).expect("df");
        let parquet_path = tmp.path().join("source.parquet");
        {
            let file = std::fs::File::create(&parquet_path).expect("create");
            ParquetWriter::new(file).finish(&mut df).expect("write");
        }

        let pks = vec!["id".to_string()];
        let source_a = "connector:aaaaaaaa";
        let source_b = "connector:bbbbbbbb";

        store
            .merge_parquet(source_a, "issues", &parquet_path, &pks)
            .await
            .expect("merge a");
        store
            .merge_parquet(source_b, "issues", &parquet_path, &pks)
            .await
            .expect("merge b");

        let tables_a = store.list_tables_by_source(source_a).await.expect("list a");
        let tables_b = store.list_tables_by_source(source_b).await.expect("list b");

        assert_eq!(tables_a.len(), 1);
        assert_eq!(tables_b.len(), 1);
        assert_eq!(tables_a[0].name, "issues");
        assert_eq!(tables_b[0].name, "issues");
        assert_ne!(tables_a[0].id, tables_b[0].id);
        assert_eq!(tables_a[0].total_rows, 3);
        assert_eq!(tables_b[0].total_rows, 3);

        // Each source gets its own on-disk directory.
        assert!(tmp.path().join(source_a).join("issues").is_dir());
        assert!(tmp.path().join(source_b).join("issues").is_dir());
    }
}

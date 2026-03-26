//! Data ingestion into manifest-based Parquet tables

use std::path::Path;

use chrono::Utc;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::{StoreError, StoreResult};
use crate::manifest::{FileEntry, Manifest};
use crate::table::table_exists;

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

/// Ingest a Parquet file into a table
pub async fn ingest_parquet(
    table_path: &Path,
    parquet_path: &Path,
    options: &IngestOptions,
) -> StoreResult<()> {
    if !parquet_path.exists() {
        return Err(StoreError::FileNotFound(parquet_path.to_path_buf()));
    }

    let exists = table_exists(table_path).await?;

    if exists {
        match options.mode {
            IngestMode::ErrorIfExists => {
                return Err(StoreError::TableAlreadyExists(
                    table_path.to_string_lossy().to_string(),
                ));
            },
            IngestMode::Ignore => {
                debug!("Table exists and mode is Ignore, skipping");
                return Ok(());
            },
            IngestMode::Overwrite => {
                // Load manifest, clear files, delete old parquet files
                let mut manifest = Manifest::load(table_path)?;
                for file_entry in &manifest.files {
                    let old_path = table_path.join(&file_entry.path);
                    if old_path.exists() {
                        std::fs::remove_file(&old_path)?;
                    }
                }
                manifest.set_files(Vec::new());
                manifest.save(table_path)?;
            },
            IngestMode::Append => {},
        }
    }

    // Create table directory and data subdir if needed
    std::fs::create_dir_all(table_path)?;
    Manifest::ensure_data_dir(table_path)?;

    let mut manifest = if exists && options.mode != IngestMode::Overwrite {
        Manifest::load(table_path)?
    } else {
        let table_name = table_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        Manifest::new(table_name)
    };

    // Generate destination filename
    let dest_relative = manifest.next_filename();
    let dest_path = table_path.join(&dest_relative);

    // Copy source parquet to data directory
    std::fs::copy(parquet_path, &dest_path)?;

    // Read parquet metadata
    let file = std::fs::File::open(&dest_path)?;
    let df = ParquetReader::new(file).finish()?;
    let num_rows = i64::try_from(df.height()).unwrap_or(0);

    if num_rows == 0 {
        debug!("Parquet file is empty, skipping ingestion");
        std::fs::remove_file(&dest_path)?;
        return Ok(());
    }

    // Extract schema from the parquet file
    let schema_json = schema_to_json(df.schema().as_ref());

    let file_size = std::fs::metadata(&dest_path)?.len();

    manifest.schema = Some(schema_json);
    manifest.add_file(FileEntry {
        path: dest_relative,
        num_rows,
        size_bytes: file_size,
        added_at: Utc::now(),
    });
    manifest.save(table_path)?;

    Ok(())
}

/// Merge (upsert) a Parquet file into a table by primary keys.
/// Uses Polars anti_join/semi_join for the merge logic.
pub async fn merge_parquet(
    table_path: &Path,
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

    // Read source parquet
    let source_path = parquet_path.to_path_buf();
    let pks: Vec<String> = primary_keys.to_vec();
    let table_path_buf = table_path.to_path_buf();

    let metrics = tokio::task::spawn_blocking(move || -> StoreResult<MergeMetrics> {
        let source_file = std::fs::File::open(&source_path)?;
        let source = ParquetReader::new(source_file).finish()?;

        if source.is_empty() {
            debug!("Parquet file is empty, skipping merge");
            return Ok(MergeMetrics::default());
        }

        let source_height = source.height();
        let exists = Manifest::exists(&table_path_buf);

        if !exists {
            // First run: create table from source
            std::fs::create_dir_all(&table_path_buf)?;
            Manifest::ensure_data_dir(&table_path_buf)?;

            let table_name = table_path_buf
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let mut manifest = Manifest::new(table_name);
            manifest.primary_keys = pks;

            let schema_json = schema_to_json(source.schema().as_ref());
            manifest.schema = Some(schema_json);

            let dest_relative = manifest.next_filename();
            let dest_path = table_path_buf.join(&dest_relative);

            write_parquet(&source, &dest_path)?;

            let file_size = std::fs::metadata(&dest_path)?.len();
            manifest.add_file(FileEntry {
                path: dest_relative,
                num_rows: i64::try_from(source_height).unwrap_or(0),
                size_bytes: file_size,
                added_at: Utc::now(),
            });
            manifest.save(&table_path_buf)?;

            return Ok(MergeMetrics {
                rows_updated: 0,
                rows_inserted: source_height,
            });
        }

        // Load existing data
        let manifest = Manifest::load(&table_path_buf)?;
        let mut existing: Option<DataFrame> = None;
        for file_entry in &manifest.files {
            let file_path = table_path_buf.join(&file_entry.path);
            let file = std::fs::File::open(&file_path)?;
            let df = ParquetReader::new(file).finish()?;
            existing = Some(match existing {
                Some(mut e) => {
                    e.vstack_mut(&df)?;
                    e
                },
                None => df,
            });
        }

        let Some(existing) = existing else {
            // No existing data, treat as first run
            let mut manifest = manifest;
            let dest_relative = manifest.next_filename();
            let dest_path = table_path_buf.join(&dest_relative);
            write_parquet(&source, &dest_path)?;

            let file_size = std::fs::metadata(&dest_path)?.len();
            manifest.schema = Some(schema_to_json(source.schema().as_ref()));
            manifest.add_file(FileEntry {
                path: dest_relative,
                num_rows: i64::try_from(source_height).unwrap_or(0),
                size_bytes: file_size,
                added_at: Utc::now(),
            });
            manifest.save(&table_path_buf)?;

            return Ok(MergeMetrics {
                rows_updated: 0,
                rows_inserted: source_height,
            });
        };

        // Polars upsert via join:
        // matched = existing rows that have matching PKs in source (these get updated)
        let matched_count = existing
            .join(
                &source,
                pks.as_slice(),
                pks.as_slice(),
                JoinArgs::new(JoinType::Semi),
                None,
            )?
            .height();

        // unchanged = existing rows NOT in source (keep as-is)
        let unchanged = existing.join(
            &source,
            pks.as_slice(),
            pks.as_slice(),
            JoinArgs::new(JoinType::Anti),
            None,
        )?;

        // result = source rows (new + updated) + unchanged existing rows
        let mut result = concat_df(&[source, unchanged])?;
        result.rechunk_mut();

        let rows_inserted = source_height.saturating_sub(matched_count);

        // Write result to new parquet file
        let mut manifest = manifest;
        let dest_relative = manifest.next_filename();
        let dest_path = table_path_buf.join(&dest_relative);
        write_parquet(&result, &dest_path)?;

        let file_size = std::fs::metadata(&dest_path)?.len();

        // Delete old parquet files
        let old_files: Vec<String> = manifest.files.iter().map(|f| f.path.clone()).collect();
        for old_file in &old_files {
            let old_path = table_path_buf.join(old_file);
            if old_path.exists() {
                std::fs::remove_file(&old_path)?;
            }
        }

        manifest.schema = Some(schema_to_json(result.schema().as_ref()));
        manifest.set_files(vec![FileEntry {
            path: dest_relative,
            num_rows: i64::try_from(result.height()).unwrap_or(0),
            size_bytes: file_size,
            added_at: Utc::now(),
        }]);
        manifest.save(&table_path_buf)?;

        info!(
            "Merge complete: {} updated, {} inserted",
            matched_count, rows_inserted
        );

        Ok(MergeMetrics {
            rows_updated: matched_count,
            rows_inserted,
        })
    })
    .await
    .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??;

    Ok(metrics)
}

/// Write a DataFrame to a parquet file
fn write_parquet(df: &DataFrame, path: &Path) -> StoreResult<()> {
    let file = std::fs::File::create(path)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;
    Ok(())
}

/// Convert a Polars Schema to a JSON value for storage in the manifest
fn schema_to_json(schema: &Schema) -> serde_json::Value {
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
}

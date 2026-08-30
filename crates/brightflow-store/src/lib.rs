//! The crate facade: `ParquetStore` pairs a root directory of Parquet data
//! files with a SQLite catalog (`StoreDb`) and resolves `(source_id,
//! table_name)` before delegating to the `ingest`/`table` helpers. This file
//! also owns the catalog-registration paths that take Parquet files as they
//! are instead of rewriting them: `register_file` (idempotent — an
//! already-registered path is skipped), `compact_partition` (merge a
//! partition's files into one, swapping the catalog rows), and
//! `register_existing_events` (one-time walk of an
//! `events/{source}/{date}/*.parquet` tree).
//!
//! Stored file paths are written root-relative, so a catalog keeps pointing at
//! its data when the workspace is copied or moved. Absolute paths written
//! before that was true still resolve, so reading accepts either form. Full-table rewrites (`replace_table_data`) take an
//! optional `expected_version` and fail with `StoreError::VersionConflict`
//! when the table's version has moved since the caller read it.

// Allow certain lints for store code:
// - similar_names: entry/dir_entry patterns are common in fs code
// - shadow_unrelated: variable shadowing for Result unwrapping is idiomatic
// - wildcard_imports: prelude imports are standard for polars
#![allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::match_same_arms,
    clippy::clone_on_ref_ptr,
    clippy::shadow_unrelated,
    clippy::shadow_reuse,
    clippy::wildcard_imports
)]

pub mod db;
mod error;
mod ingest;
pub mod migrate;
mod models;
pub mod pool;
pub mod row;
pub mod scan;
mod stats;
pub(crate) mod table;

pub use error::{StoreError, StoreResult};
// Downstream crates use SQLite through these re-exports (including `rusqlite`
// itself) so the whole workspace shares one driver version by construction.
pub use deadpool_sqlite::rusqlite;
pub use ingest::{IngestMode, IngestOptions, MergeMetrics};
pub use migrate::{migrate, MigrateError, Migration};
pub use models::{
    ActionLogRow, AgentRunRow, ColumnSemanticRow, EnrichmentCacheRow, EnrichmentFunctionRow,
    EnrichmentFunctionVersionRow, EnrichmentRunRow, FileColumnStatRow, InsightHistoryRow,
    InsightRunRow, InsightStateRow, InsightSuppressionRow, SourceRow, TableAnalysisSettingsRow,
    TableRow, TaxonomyCategoryRow, UnresolvedSubjectRow,
};
pub use pool::{open_pool, SqliteError, SqlitePool, SqlitePoolProfile};
pub use row::{execute, fetch_all, fetch_one, fetch_optional, FromRow};
pub use scan::ScanFilter;
pub use stats::extract_file_column_stats;
pub use table::{TableInfo, TableRef};

use std::path::{Path, PathBuf};

use db::StoreDb;
use polars::prelude::*;
use tracing::info;

/// Parquet store for managing tables
pub struct ParquetStore {
    /// Root path for all tables
    root_path: PathBuf,
    /// SQLite metadata database
    db: StoreDb,
}

impl ParquetStore {
    /// Create a new `ParquetStore` with the given root path and database URL
    pub async fn new(root_path: impl Into<PathBuf>, database_url: &str) -> StoreResult<Self> {
        let db = StoreDb::new(database_url).await?;
        let root = root_path.into();

        Ok(Self {
            root_path: root,
            db,
        })
    }

    /// Get the root path of the store
    #[must_use]
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// List all tables in the store
    pub async fn list_tables(&self) -> StoreResult<Vec<TableRef>> {
        table::list_tables(&self.db, &self.root_path).await
    }

    /// List tables belonging to a specific source
    pub async fn list_tables_by_source(&self, source_id: &str) -> StoreResult<Vec<TableRow>> {
        self.db.list_tables_by_source(source_id).await
    }

    /// Get information about a table
    pub async fn table_info(&self, source_id: &str, name: &str) -> StoreResult<TableInfo> {
        table::get_table_info(&self.db, source_id, name, &self.root_path).await
    }

    /// Read a table as a Polars DataFrame
    pub async fn read_table(&self, source_id: &str, name: &str) -> StoreResult<DataFrame> {
        info!("Reading table '{}' for source '{}'", name, source_id);
        table::read_table(&self.db, source_id, name, &self.root_path).await
    }

    /// Get the parquet file paths for a table (for lazy scan mode)
    pub async fn get_table_parquet_paths(
        &self,
        source_id: &str,
        name: &str,
    ) -> StoreResult<Vec<PathBuf>> {
        table::get_parquet_paths(&self.db, source_id, name, &self.root_path).await
    }

    /// Ingest a Parquet file into a table
    ///
    /// If the table doesn't exist, it will be created with the schema from the Parquet file.
    /// If it exists, data will be appended.
    pub async fn ingest_parquet(
        &self,
        source_id: &str,
        table_name: &str,
        parquet_path: impl AsRef<Path>,
        options: Option<IngestOptions>,
    ) -> StoreResult<TableInfo> {
        let ingest_options = options.unwrap_or_default();

        info!(
            "Ingesting parquet {:?} into table '{}' for source '{}'",
            parquet_path.as_ref(),
            table_name,
            source_id,
        );

        ingest::ingest_parquet(
            &self.db,
            &self.root_path,
            source_id,
            table_name,
            parquet_path.as_ref(),
            &ingest_options,
        )
        .await?;
        self.table_info(source_id, table_name).await
    }

    /// Merge (upsert) a Parquet file into a table by primary key.
    /// Uses Polars join operations to update existing rows and insert new ones.
    pub async fn merge_parquet(
        &self,
        source_id: &str,
        table_name: &str,
        parquet_path: impl AsRef<Path>,
        primary_keys: &[String],
    ) -> StoreResult<MergeMetrics> {
        info!(
            "Merging parquet {:?} into table '{}' for source '{}' with PKs {:?}",
            parquet_path.as_ref(),
            table_name,
            source_id,
            primary_keys
        );
        ingest::merge_parquet(
            &self.db,
            &self.root_path,
            source_id,
            table_name,
            parquet_path.as_ref(),
            primary_keys,
        )
        .await
    }

    /// Replace a table's entire contents with a DataFrame (one consolidated
    /// file). `expected_version` enables an optimistic concurrency check —
    /// see `StoreError::VersionConflict`.
    pub async fn replace_table_data(
        &self,
        source_id: &str,
        table_name: &str,
        df: DataFrame,
        expected_version: Option<i64>,
    ) -> StoreResult<()> {
        info!(
            "Replacing table '{}' data for source '{}'",
            table_name, source_id
        );
        ingest::replace_table_data(
            &self.db,
            &self.root_path,
            source_id,
            table_name,
            df,
            expected_version,
        )
        .await
    }

    /// Delete a single table for a source
    pub async fn delete_table(&self, source_id: &str, name: &str) -> StoreResult<()> {
        info!("Deleting table '{}' for source '{}'", name, source_id);
        table::delete_table(&self.db, source_id, name, &self.root_path).await
    }

    /// Delete all tables and on-disk parquet files for a source.
    /// Safe to call when the source has no data (missing directory is ignored).
    pub async fn delete_source_data(&self, source_id: &str) -> StoreResult<u64> {
        info!("Deleting all store data for source '{}'", source_id);
        let deleted = self.db.delete_tables_by_source(source_id).await?;
        let dir = self.root_path.join(source_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(deleted)
    }

    // =====================================================
    // Partitioned table operations
    // =====================================================

    /// Resolve a stored file path: absolute paths pass through, relative paths
    /// are resolved against `root_path`. Backward-compatible with existing data.
    fn resolve_file_path(&self, stored_path: &str) -> PathBuf {
        let p = Path::new(stored_path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root_path.join(p)
        }
    }

    /// Register an existing Parquet file in the catalog.
    ///
    /// Idempotent: skips if the file path is already registered.
    pub async fn register_file(
        &self,
        source_id: &str,
        table_name: &str,
        file_path: &Path,
        partition_values: &[(&str, &str)],
        partition_columns: Option<&[&str]>,
        stats: Option<Vec<FileColumnStatRow>>,
    ) -> StoreResult<()> {
        let pc_json = partition_columns.map(|cols| serde_json::to_string(cols).unwrap_or_default());
        let table = self
            .db
            .get_or_create_table(table_name, pc_json.as_deref(), source_id)
            .await?;

        // Store the path relative to the store root so the catalog survives the
        // workspace being copied or moved. Absolute paths written by older
        // versions still resolve — `resolve_path` passes them through — so this
        // needs no migration, but nothing new should be written that way.
        let abs_path = std::fs::canonicalize(file_path)?;
        let path_str = table::relativize_path(&self.root_path, &abs_path)
            .unwrap_or_else(|| abs_path.clone())
            .to_string_lossy()
            .to_string();
        if self.db.is_file_registered(&table.id, &path_str).await? {
            return Ok(());
        }

        // Get file size from filesystem
        let metadata = std::fs::metadata(file_path)?;
        let size_bytes = i64::try_from(metadata.len()).unwrap_or(0);

        // Get row count from Parquet metadata (fast, no full read)
        let num_rows = {
            let fp = file_path.to_path_buf();
            tokio::task::spawn_blocking(move || -> StoreResult<i64> {
                let file = std::fs::File::open(&fp)?;
                let reader = ParquetReader::new(file);
                let df = reader.finish()?;
                Ok(i64::try_from(df.height()).unwrap_or(0))
            })
            .await
            .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??
        };

        // Register the file
        let file_row = self
            .db
            .add_table_file(&table.id, &path_str, num_rows, size_bytes)
            .await?;

        // Add partition values
        if !partition_values.is_empty() {
            self.db
                .add_file_partitions(&file_row.id, partition_values)
                .await?;
        }

        // Add column stats
        if let Some(mut file_stats) = stats {
            // Set the correct file_id (caller may have used a placeholder)
            for s in &mut file_stats {
                s.file_id.clone_from(&file_row.id);
            }
            self.db.add_file_column_stats(&file_stats).await?;
        } else {
            // Extract stats from the file
            let fp = file_path.to_path_buf();
            let fid = file_row.id.clone();
            let file_stats =
                tokio::task::spawn_blocking(move || -> StoreResult<Vec<FileColumnStatRow>> {
                    let file = std::fs::File::open(&fp)?;
                    let df = ParquetReader::new(file).finish()?;
                    Ok(extract_file_column_stats(&df, &fid))
                })
                .await
                .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??;
            self.db.add_file_column_stats(&file_stats).await?;
        }

        // Update table total_rows
        let new_total = table.total_rows + num_rows;
        self.db
            .update_table_meta(&table.id, None, None, new_total)
            .await?;

        Ok(())
    }

    /// Scan a table with filters, returning a pruned `LazyFrame`.
    ///
    /// Returns `Ok(None)` if the table doesn't exist or no files match.
    pub async fn scan_table(
        &self,
        source_id: &str,
        table_name: &str,
        filters: &[ScanFilter],
    ) -> StoreResult<Option<LazyFrame>> {
        let Some(table) = self.db.get_table(source_id, table_name).await? else {
            return Ok(None);
        };

        let files = self.db.query_pruned_files(&table.id, filters).await?;
        if files.is_empty() {
            return Ok(None);
        }

        let paths: Arc<[PathBuf]> = files
            .iter()
            .map(|f| self.resolve_file_path(&f.path))
            .collect();

        // Use scan_parquet_files for multiple paths
        let lf = LazyFrame::scan_parquet_files(paths, ScanArgsParquet::default())?;

        Ok(Some(lf))
    }

    /// Compact all files in a partition into a single file.
    ///
    /// Returns the number of files that were merged (0 if nothing to compact).
    pub async fn compact_partition(
        &self,
        source_id: &str,
        table_name: &str,
        partition_key: &str,
        partition_value: &str,
    ) -> StoreResult<usize> {
        let table_id = self.table_id(source_id, table_name).await?;

        let files = self
            .db
            .get_partition_file_ids(&table_id, partition_key, partition_value)
            .await?;

        if files.len() <= 1 {
            return Ok(0);
        }

        let file_count = files.len();
        let old_paths: Vec<PathBuf> = files
            .iter()
            .map(|f| self.resolve_file_path(&f.path))
            .collect();
        let old_ids: Vec<String> = files.iter().map(|f| f.id.clone()).collect();

        // Read and concatenate all files
        let paths_clone = old_paths.clone();
        let (merged_df, merged_path) =
            tokio::task::spawn_blocking(move || -> StoreResult<(DataFrame, PathBuf)> {
                let mut frames = Vec::new();
                for path in &paths_clone {
                    let file = std::fs::File::open(path)?;
                    let df = ParquetReader::new(file).finish()?;
                    frames.push(df);
                }
                let mut merged = ingest::concat_df(&frames)?;
                merged.rechunk_mut();

                // Write to a new file in the same directory as the first file
                let dir = paths_clone
                    .first()
                    .and_then(|p| p.parent())
                    .unwrap_or_else(|| Path::new("."));
                std::fs::create_dir_all(dir)?;
                let new_filename = format!("{}.parquet", uuid::Uuid::now_v7());
                let new_path = dir.join(new_filename);

                let file = std::fs::File::create(&new_path)?;
                ParquetWriter::new(file)
                    .with_compression(ParquetCompression::Zstd(None))
                    .finish(&mut merged)?;

                Ok((merged, new_path))
            })
            .await
            .map_err(|e| StoreError::Other(format!("Task join error: {e}")))??;

        let merged_path_str = merged_path.to_string_lossy().to_string();
        let num_rows = i64::try_from(merged_df.height()).unwrap_or(0);
        let size_bytes = i64::try_from(std::fs::metadata(&merged_path)?.len()).unwrap_or(0);

        // In a transaction-like sequence: delete old, insert new
        self.db.delete_files_by_ids(&old_ids).await?;

        let new_file = self
            .db
            .add_table_file(&table_id, &merged_path_str, num_rows, size_bytes)
            .await?;

        self.db
            .add_file_partitions(&new_file.id, &[(partition_key, partition_value)])
            .await?;

        let file_stats = extract_file_column_stats(&merged_df, &new_file.id);
        self.db.add_file_column_stats(&file_stats).await?;

        // Recompute total rows for the table
        let all_files = self.db.list_table_files(&table_id).await?;
        let total: i64 = all_files.iter().map(|f| f.num_rows).sum();
        self.db
            .update_table_meta(&table_id, None, None, total)
            .await?;

        // Delete old physical files
        for path in &old_paths {
            if path.exists() {
                drop(std::fs::remove_file(path));
            }
        }

        info!(
            "Compacted {} files in {}/{}={} → 1 file ({} rows)",
            file_count, table_name, partition_key, partition_value, num_rows
        );

        Ok(file_count)
    }

    /// List the distinct values of a partition key for a table, from the
    /// `file_partitions` catalog rows (not inferred from file paths).
    ///
    /// Errors with `TableNotFound` for an unknown table; returns an empty
    /// vec for a known table with no files under that key.
    pub async fn list_partition_values(
        &self,
        source_id: &str,
        table_name: &str,
        partition_key: &str,
    ) -> StoreResult<Vec<String>> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db
            .list_partition_values(&table_id, partition_key)
            .await
    }

    /// Resolve (source_id, table_name) to the table's id, or TableNotFound.
    async fn table_id(&self, source_id: &str, table_name: &str) -> StoreResult<String> {
        Ok(self
            .db
            .get_table(source_id, table_name)
            .await?
            .ok_or_else(|| StoreError::TableNotFound(table_name.to_string()))?
            .id)
    }

    // =====================================================
    // Column Semantics (high-level, resolves table name → id)
    // =====================================================

    /// Get column semantics overrides for a table by (source_id, name).
    pub async fn get_column_semantics(
        &self,
        source_id: &str,
        table_name: &str,
    ) -> StoreResult<Vec<ColumnSemanticRow>> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db.get_column_semantics(&table_id).await
    }

    /// Upsert a single column semantic override by (source_id, table name).
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_column_semantic(
        &self,
        source_id: &str,
        table_name: &str,
        column_name: &str,
        role: &str,
        is_kpi: bool,
        polarity: &str,
        label: Option<&str>,
        description: Option<&str>,
    ) -> StoreResult<ColumnSemanticRow> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db
            .upsert_column_semantic(
                &table_id,
                column_name,
                role,
                is_kpi,
                polarity,
                label,
                description,
            )
            .await
    }

    /// Batch upsert column semantics for a table by (source_id, name).
    pub async fn upsert_column_semantics_batch(
        &self,
        source_id: &str,
        table_name: &str,
        rows: &[ColumnSemanticRow],
    ) -> StoreResult<()> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db.upsert_column_semantics_batch(&table_id, rows).await
    }

    /// Delete a single column semantic override by (source_id, table name).
    pub async fn delete_column_semantic(
        &self,
        source_id: &str,
        table_name: &str,
        column_name: &str,
    ) -> StoreResult<bool> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db.delete_column_semantic(&table_id, column_name).await
    }

    /// Delete all column semantic overrides for a table by (source_id, name).
    pub async fn delete_all_column_semantics(
        &self,
        source_id: &str,
        table_name: &str,
    ) -> StoreResult<u64> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db.delete_all_column_semantics(&table_id).await
    }

    /// Get table analysis settings by (source_id, table name).
    pub async fn get_table_settings(
        &self,
        source_id: &str,
        table_name: &str,
    ) -> StoreResult<Option<TableAnalysisSettingsRow>> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db.get_table_settings(&table_id).await
    }

    /// Upsert table analysis settings by (source_id, table name).
    pub async fn upsert_table_settings(
        &self,
        source_id: &str,
        table_name: &str,
        display_name: Option<&str>,
        description: Option<&str>,
        time_granularity: Option<&str>,
        comparison_periods: Option<i32>,
    ) -> StoreResult<TableAnalysisSettingsRow> {
        let table_id = self.table_id(source_id, table_name).await?;
        self.db
            .upsert_table_settings(
                &table_id,
                display_name,
                description,
                time_granularity,
                comparison_periods,
            )
            .await
    }

    /// Get the internal StoreDb (for seeding operations that need direct access).
    pub fn db(&self) -> &StoreDb {
        &self.db
    }

    /// One-time migration: walk an events directory and register all Parquet files.
    ///
    /// Expected layout: `events_path/{source_id}/{date}/*.parquet`
    /// Idempotent — skips already-registered files.
    pub async fn register_existing_events(&self, events_path: &Path) -> StoreResult<usize> {
        if !events_path.exists() {
            return Ok(0);
        }

        let mut count = 0usize;

        // Walk: events_path / {source_id} / {date} / *.parquet
        let source_dirs = std::fs::read_dir(events_path)?;
        for source_entry in source_dirs.flatten() {
            let source_path = source_entry.path();
            if !source_path.is_dir() {
                continue;
            }
            let source_id = source_entry.file_name().to_string_lossy().to_string();
            let table_name = brightflow_core::events_table_name(&source_id);

            let date_dirs = std::fs::read_dir(&source_path)?;
            for date_entry in date_dirs.flatten() {
                let date_path = date_entry.path();
                if !date_path.is_dir() {
                    continue;
                }
                let date = date_entry.file_name().to_string_lossy().to_string();

                let parquet_files = std::fs::read_dir(&date_path)?;
                for file_entry in parquet_files.flatten() {
                    let file_path = file_entry.path();
                    if file_path.extension().is_some_and(|ext| ext == "parquet") {
                        self.register_file(
                            &brightflow_core::web_source_id(&source_id),
                            &table_name,
                            &file_path,
                            &[("date", &date)],
                            Some(&["date"]),
                            None,
                        )
                        .await?;
                        count += 1;
                    }
                }
            }
        }

        info!("Registered {count} existing event files");
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_store() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        let store = ParquetStore::new(tmp.path(), &db_url)
            .await
            .expect("failed to create store");
        assert_eq!(store.root_path(), tmp.path());
    }

    #[tokio::test]
    async fn test_list_empty_store() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        let store = ParquetStore::new(tmp.path(), &db_url)
            .await
            .expect("failed to create store");
        let tables = store.list_tables().await.expect("failed to list tables");
        assert!(tables.is_empty());
    }
}

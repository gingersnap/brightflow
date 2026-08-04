//! Brightflow Store - SQLite-backed Parquet storage (Litehouse)
//!
//! This crate provides Brightflow's lakehouse implementation using
//! SQLite metadata + Parquet data files.

// Allow certain lints for store code:
// - similar_names: entry/dir_entry patterns are common in fs code
// - shadow_unrelated: variable shadowing for Result unwrapping is idiomatic
// - wildcard_imports: prelude imports are standard for polars
// - unused_async: async kept for API compatibility
#![allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::indexing_slicing,
    clippy::cast_sign_loss,
    clippy::match_same_arms,
    clippy::clone_on_ref_ptr,
    clippy::shadow_unrelated,
    clippy::shadow_reuse,
    clippy::wildcard_imports,
    clippy::unused_async
)]

pub mod db;
mod error;
mod ingest;
mod models;
pub mod scan;
pub mod sqlite;
mod stats;
mod table;

pub use error::{StoreError, StoreResult};
pub use ingest::{IngestMode, IngestOptions, MergeMetrics};
pub use models::{
    ActionLogRow, AgentRunRow, ClusterEditRow, ColumnSemanticRow, DocumentLabelRow,
    DocumentLabelWithName, EnrichmentCacheRow, EnrichmentFunctionRow, EnrichmentFunctionVersionRow,
    EnrichmentRunRow, ExcludedTermRow, FileColumnStatRow, InsightHistoryRow, InsightRunRow,
    InsightStateRow, InsightSuppressionRow, SourceRow, TableAnalysisSettingsRow,
    TableEnrichmentSettingsRow, TableRow, TaxonomyCategoryRow,
};
pub use scan::ScanFilter;
pub use sqlite::{open_sqlite_pool, SqlitePoolProfile};
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

        let abs_path = std::fs::canonicalize(file_path)?;
        let path_str = abs_path.to_string_lossy().to_string();
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
                let dir = paths_clone[0].parent().unwrap_or_else(|| Path::new("."));
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

    async fn temp_store(tmp: &TempDir) -> ParquetStore {
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        ParquetStore::new(tmp.path(), &db_url)
            .await
            .expect("failed to create store")
    }

    /// Bulk approve applies proposals **oldest first**, and this is the accessor
    /// that guarantees it.
    ///
    /// The ordering is a correctness requirement, not a preference: an agent
    /// proposes `define_taxonomy_category` before the `label_document` that
    /// names it, so applying newest-first would fail every label with "not a
    /// category in this table's taxonomy". The neighbouring `list_actions`
    /// (the audit feed) is deliberately DESC, so this is an easy thing to get
    /// backwards by copy-paste.
    #[tokio::test]
    async fn list_proposed_actions_is_oldest_first_and_only_proposed() {
        let tmp = TempDir::new().expect("temp dir");
        let store = temp_store(&tmp).await;
        let db = store.db();

        // Insert in creation order: two proposals, one already applied, one more.
        for (request_id, kind, status) in [
            ("r1", "define_taxonomy_category", "proposed"),
            ("r2", "label_document", "proposed"),
            ("r3", "rename_cluster", "applied"),
            ("r4", "label_document", "proposed"),
        ] {
            let row = db
                .insert_action(request_id, "agent", Some(1), kind, "{}", status, 0)
                .await
                .expect("insert")
                .expect("no request_id conflict");
            if status == "applied" {
                db.update_action_result(row.id, "applied", None, None, 0)
                    .await
                    .expect("mark applied");
            }
        }

        let proposed = db.list_proposed_actions().await.expect("list");
        assert_eq!(proposed.len(), 3, "the applied action must not be returned");

        let ids: Vec<i64> = proposed.iter().map(|r| r.id).collect();
        let mut ascending = ids.clone();
        ascending.sort_unstable();
        assert_eq!(
            ids, ascending,
            "must be oldest-first, or dependent proposals fail"
        );
        assert_eq!(
            proposed.first().map(|r| r.action_kind.as_str()),
            Some("define_taxonomy_category"),
            "the category must be applied before the label that names it"
        );

        assert_eq!(db.count_proposed_actions().await.expect("count"), 3);
    }

    #[tokio::test]
    async fn count_proposed_actions_is_zero_on_a_fresh_store() {
        let tmp = TempDir::new().expect("temp dir");
        let store = temp_store(&tmp).await;
        assert_eq!(store.db().count_proposed_actions().await.expect("count"), 0);
        assert!(store
            .db()
            .list_proposed_actions()
            .await
            .expect("list")
            .is_empty());
    }

    /// Deleting a category must take its row labels with it — the FK cascade
    /// only fires because the pool sets `foreign_keys(true)`, which is easy to
    /// lose in a refactor and silent when it breaks.
    #[tokio::test]
    async fn deleting_a_category_cascades_to_its_labels() {
        let tmp = TempDir::new().expect("temp dir");
        let store = temp_store(&tmp).await;
        let db = store.db();

        let cat = db
            .upsert_taxonomy_category("t1", "auth failure", Some("cannot log in"), 0)
            .await
            .expect("define");
        db.set_document_labels("t1", "row-1", &[cat.id], "human", 0)
            .await
            .expect("label");
        assert_eq!(db.count_labelled_rows("t1").await.expect("count"), 1);

        assert!(db.delete_taxonomy_category(cat.id).await.expect("delete"));
        assert_eq!(
            db.count_labelled_rows("t1").await.expect("count"),
            0,
            "labels must cascade away with their category"
        );
    }

    /// The undo path reinserts a deleted category under its ORIGINAL id so the
    /// restored labels still point at it.
    #[tokio::test]
    async fn recreating_a_category_restores_its_labels() {
        let tmp = TempDir::new().expect("temp dir");
        let store = temp_store(&tmp).await;
        let db = store.db();

        let cat = db
            .upsert_taxonomy_category("t1", "data loss", None, 0)
            .await
            .expect("define");
        db.set_document_labels("t1", "row-1", &[cat.id], "agent", 0)
            .await
            .expect("label");

        let snapshot: Vec<(String, String, i64)> = db
            .get_document_labels_for_category(cat.id)
            .await
            .expect("snapshot")
            .into_iter()
            .map(|l| (l.row_id, l.source, l.created_at))
            .collect();
        db.delete_taxonomy_category(cat.id).await.expect("delete");

        db.recreate_taxonomy_category(cat.id, "t1", "data loss", None, 0, &snapshot)
            .await
            .expect("recreate");

        let labels = db.get_document_labels("t1").await.expect("labels");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels.first().map(|l| l.name.as_str()), Some("data loss"));
        assert_eq!(labels.first().map(|l| l.category_id), Some(cat.id));
    }

    /// Restoring under an id whose NAME has since been taken must fail loudly
    /// rather than roll back with a raw constraint error.
    #[tokio::test]
    async fn recreating_a_category_reports_a_name_clash() {
        let tmp = TempDir::new().expect("temp dir");
        let store = temp_store(&tmp).await;
        let db = store.db();

        let original = db
            .upsert_taxonomy_category("t1", "billing", None, 0)
            .await
            .expect("define");
        db.delete_taxonomy_category(original.id)
            .await
            .expect("delete");
        // The same name is re-defined and gets a NEW id.
        let replacement = db
            .upsert_taxonomy_category("t1", "billing", None, 0)
            .await
            .expect("redefine");
        assert_ne!(replacement.id, original.id);

        let err = db
            .recreate_taxonomy_category(original.id, "t1", "billing", None, 0, &[])
            .await
            .expect_err("must refuse rather than violate UNIQUE(table_id, name)");
        assert!(
            err.to_string().contains("billing"),
            "the error must name the clash: {err}"
        );
    }

    #[tokio::test]
    async fn set_document_labels_replaces_the_whole_set() {
        let tmp = TempDir::new().expect("temp dir");
        let store = temp_store(&tmp).await;
        let db = store.db();

        let a = db
            .upsert_taxonomy_category("t1", "a", None, 0)
            .await
            .expect("a");
        let b = db
            .upsert_taxonomy_category("t1", "b", None, 0)
            .await
            .expect("b");

        db.set_document_labels("t1", "row-1", &[a.id, b.id], "agent", 0)
            .await
            .expect("set both");
        assert_eq!(
            db.get_labels_for_row("t1", "row-1")
                .await
                .expect("get")
                .len(),
            2
        );

        // Replace, not merge.
        db.set_document_labels("t1", "row-1", &[a.id], "human", 0)
            .await
            .expect("replace");
        let rows = db.get_labels_for_row("t1", "row-1").await.expect("get");
        assert_eq!(rows.len(), 1, "the removed label must be gone, not merged");
        assert_eq!(rows.first().map(|r| r.source.as_str()), Some("human"));

        // An empty set clears the row — a real prior state, not a no-op.
        db.set_document_labels("t1", "row-1", &[], "human", 0)
            .await
            .expect("clear");
        assert!(db
            .get_labels_for_row("t1", "row-1")
            .await
            .expect("get")
            .is_empty());
    }
}

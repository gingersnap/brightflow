//! Catalog queries: tables, their files and stats, partitions, column
//! semantics, analysis settings, and registered upload sources — the
//! "what data exists and what does it mean" half of the store.

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::{
    ColumnSemanticRow, ColumnStatRow, FileColumnStatRow, SourceRow, TableAnalysisSettingsRow,
    TableFileRow, TableRow,
};
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};
use crate::scan::ScanFilter;
use rusqlite::{params, params_from_iter};

impl StoreDb {
    // =====================================================
    // Tables CRUD
    // =====================================================

    pub async fn create_table(&self, name: &str, source_id: &str) -> StoreResult<TableRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let name = name.to_owned();
        let source_id = source_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<TableRow, _>(
                    conn,
                    r"INSERT INTO tables (id, name, source_id)
                      VALUES (?, ?, ?)
                      RETURNING *",
                    params![id, name, source_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_table(&self, source_id: &str, name: &str) -> StoreResult<Option<TableRow>> {
        let source_id = source_id.to_owned();
        let name = name.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TableRow, _>(
                    conn,
                    "SELECT * FROM tables WHERE source_id = ? AND name = ?",
                    params![source_id, name],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_table_by_id(&self, id: &str) -> StoreResult<Option<TableRow>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TableRow, _>(
                    conn,
                    "SELECT * FROM tables WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_tables(&self) -> StoreResult<Vec<TableRow>> {
        let rows = self
            .pool
            .call(|conn| fetch_all::<TableRow, _>(conn, "SELECT * FROM tables ORDER BY name", []))
            .await?;
        Ok(rows)
    }

    pub async fn list_tables_by_source(&self, source_id: &str) -> StoreResult<Vec<TableRow>> {
        let source_id = source_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<TableRow, _>(
                    conn,
                    "SELECT * FROM tables WHERE source_id = ? ORDER BY name",
                    params![source_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn update_table_meta(
        &self,
        id: &str,
        schema_json: Option<&str>,
        primary_keys: Option<&str>,
        total_rows: i64,
    ) -> StoreResult<Option<TableRow>> {
        let id = id.to_owned();
        let schema_json = schema_json.map(ToOwned::to_owned);
        let primary_keys = primary_keys.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TableRow, _>(
                    conn,
                    r"UPDATE tables
                      SET schema_json = COALESCE(?, schema_json),
                          primary_keys = COALESCE(?, primary_keys),
                          total_rows = ?,
                          version = version + 1,
                          updated_at = datetime('now')
                      WHERE id = ?
                      RETURNING *",
                    params![schema_json, primary_keys, total_rows, id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn delete_table(&self, source_id: &str, name: &str) -> StoreResult<bool> {
        let source_id = source_id.to_owned();
        let name = name.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM tables WHERE source_id = ? AND name = ?",
                    params![source_id, name],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    /// Delete all tables belonging to a source. Cascades to table_files,
    /// column_stats, file_partitions, file_column_stats, column_semantics,
    /// and table_analysis_settings via `ON DELETE CASCADE`.
    pub async fn delete_tables_by_source(&self, source_id: &str) -> StoreResult<u64> {
        let source_id = source_id.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM tables WHERE source_id = ?",
                    params![source_id],
                )
            })
            .await?;
        Ok(affected)
    }

    // =====================================================
    // Table Files CRUD
    // =====================================================

    pub async fn add_table_file(
        &self,
        table_id: &str,
        path: &str,
        num_rows: i64,
        size_bytes: i64,
    ) -> StoreResult<TableFileRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let table_id = table_id.to_owned();
        let path = path.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<TableFileRow, _>(
                    conn,
                    r"INSERT INTO table_files (id, table_id, path, num_rows, size_bytes)
                      VALUES (?, ?, ?, ?, ?)
                      RETURNING *",
                    params![id, table_id, path, num_rows, size_bytes],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_table_files(&self, table_id: &str) -> StoreResult<Vec<TableFileRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<TableFileRow, _>(
                    conn,
                    "SELECT * FROM table_files WHERE table_id = ? ORDER BY added_at",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Replace all files for a table in a single transaction.
    /// Used by merge operations that rewrite to a single consolidated file.
    pub async fn replace_table_files(
        &self,
        table_id: &str,
        new_files: &[(String, i64, i64)], // (path, num_rows, size_bytes)
    ) -> StoreResult<Vec<TableFileRow>> {
        let tid = table_id.to_owned();
        let new_files = new_files.to_vec();
        self.pool
            .transaction(move |tx| {
                execute(
                    tx,
                    "DELETE FROM table_files WHERE table_id = ?",
                    params![tid],
                )?;
                let mut stmt = tx.prepare(
                    r"INSERT INTO table_files (id, table_id, path, num_rows, size_bytes)
                      VALUES (?, ?, ?, ?, ?)",
                )?;
                for (path, num_rows, size_bytes) in &new_files {
                    let id = uuid::Uuid::now_v7().to_string();
                    stmt.execute(params![id, tid, path, num_rows, size_bytes])?;
                }
                Ok(())
            })
            .await?;
        self.list_table_files(table_id).await
    }

    pub async fn delete_table_files(&self, table_id: &str) -> StoreResult<u64> {
        let table_id = table_id.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM table_files WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(affected)
    }

    // =====================================================
    // Column Stats
    // =====================================================

    pub async fn upsert_column_stats(
        &self,
        table_id: &str,
        stats: &[ColumnStatRow],
    ) -> StoreResult<()> {
        let table_id = table_id.to_owned();
        let stats = stats.to_vec();
        self.pool
            .transaction(move |tx| {
                // Clear existing stats for this table before inserting new ones
                execute(
                    tx,
                    "DELETE FROM table_column_stats WHERE table_id = ?",
                    params![table_id],
                )?;
                let mut stmt = tx.prepare(
                    r"INSERT INTO table_column_stats (table_id, column_name, min_value, max_value, null_count)
                      VALUES (?, ?, ?, ?, ?)",
                )?;
                for stat in &stats {
                    stmt.execute(params![
                        table_id,
                        stat.column_name,
                        stat.min_value,
                        stat.max_value,
                        stat.null_count
                    ])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_column_stats(&self, table_id: &str) -> StoreResult<Vec<ColumnStatRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ColumnStatRow, _>(
                    conn,
                    "SELECT * FROM table_column_stats WHERE table_id = ? ORDER BY column_name",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    // =====================================================
    // Partitioned Tables (file-level partitions + stats)
    // =====================================================

    /// Idempotent get-or-create: returns existing table or creates a new one.
    pub async fn get_or_create_table(
        &self,
        name: &str,
        partition_columns: Option<&str>,
        source_id: &str,
    ) -> StoreResult<TableRow> {
        if let Some(row) = self.get_table(source_id, name).await? {
            return Ok(row);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let name = name.to_owned();
        let partition_columns = partition_columns.map(ToOwned::to_owned);
        let source_id = source_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<TableRow, _>(
                    conn,
                    r"INSERT INTO tables (id, name, partition_columns, source_id)
                      VALUES (?, ?, ?, ?)
                      RETURNING *",
                    params![id, name, partition_columns, source_id],
                )
            })
            .await?;
        Ok(row)
    }

    /// Batch insert partition key/value pairs for a file.
    pub async fn add_file_partitions(
        &self,
        file_id: &str,
        partitions: &[(&str, &str)],
    ) -> StoreResult<()> {
        let file_id = file_id.to_owned();
        let partitions: Vec<(String, String)> = partitions
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        self.pool
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    r"INSERT OR IGNORE INTO file_partitions (file_id, partition_key, partition_value)
                      VALUES (?, ?, ?)",
                )?;
                for (key, value) in &partitions {
                    stmt.execute(params![file_id, key, value])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Batch insert per-file column statistics.
    pub async fn add_file_column_stats(&self, stats: &[FileColumnStatRow]) -> StoreResult<()> {
        let stats = stats.to_vec();
        self.pool
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    r"INSERT OR IGNORE INTO file_column_stats (file_id, column_name, min_value, max_value, null_count)
                      VALUES (?, ?, ?, ?, ?)",
                )?;
                for stat in &stats {
                    stmt.execute(params![
                        stat.file_id,
                        stat.column_name,
                        stat.min_value,
                        stat.max_value,
                        stat.null_count
                    ])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Check if a file path is already registered for a table.
    pub async fn is_file_registered(&self, table_id: &str, path: &str) -> StoreResult<bool> {
        let table_id = table_id.to_owned();
        let path = path.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<(i64,), _>(
                    conn,
                    "SELECT COUNT(*) FROM table_files WHERE table_id = ? AND path = ?",
                    params![table_id, path],
                )
            })
            .await?;
        Ok(row.is_some_and(|(count,)| count > 0))
    }

    /// Core pruning query: returns only files matching the given filters.
    pub async fn query_pruned_files(
        &self,
        table_id: &str,
        filters: &[ScanFilter],
    ) -> StoreResult<Vec<TableFileRow>> {
        let (sql, bind_values) = crate::scan::build_pruning_query(table_id, filters);
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<TableFileRow, _>(conn, &sql, params_from_iter(bind_values))
            })
            .await?;
        Ok(rows)
    }

    /// Get file IDs in a specific partition (for compaction).
    pub async fn get_partition_file_ids(
        &self,
        table_id: &str,
        key: &str,
        value: &str,
    ) -> StoreResult<Vec<TableFileRow>> {
        let table_id = table_id.to_owned();
        let key = key.to_owned();
        let value = value.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<TableFileRow, _>(
                    conn,
                    r"SELECT tf.id, tf.table_id, tf.path, tf.num_rows, tf.size_bytes, tf.added_at
                      FROM table_files tf
                      INNER JOIN file_partitions fp ON fp.file_id = tf.id
                      WHERE tf.table_id = ?
                        AND fp.partition_key = ?
                        AND fp.partition_value = ?
                      ORDER BY tf.added_at",
                    params![table_id, key, value],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Distinct values of one partition key across a table's files, sorted
    /// ascending. Reads the `file_partitions` rows — the same join
    /// `get_partition_file_ids` uses — rather than inferring from file paths.
    pub async fn list_partition_values(
        &self,
        table_id: &str,
        key: &str,
    ) -> StoreResult<Vec<String>> {
        let table_id = table_id.to_owned();
        let key = key.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<(String,), _>(
                    conn,
                    r"SELECT DISTINCT fp.partition_value
                      FROM file_partitions fp
                      INNER JOIN table_files tf ON tf.id = fp.file_id
                      WHERE tf.table_id = ?
                        AND fp.partition_key = ?
                      ORDER BY fp.partition_value",
                    params![table_id, key],
                )
            })
            .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    /// Delete specific file records by ID (cascade cleans up partitions + stats).
    pub async fn delete_files_by_ids(&self, file_ids: &[String]) -> StoreResult<()> {
        let file_ids = file_ids.to_vec();
        self.pool
            .call(move |conn| {
                let mut stmt = conn.prepare("DELETE FROM table_files WHERE id = ?")?;
                for id in &file_ids {
                    stmt.execute(params![id])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    // =====================================================
    // Column Semantics CRUD
    // =====================================================

    pub async fn get_column_semantics(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<ColumnSemanticRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<ColumnSemanticRow, _>(
                    conn,
                    "SELECT * FROM column_semantics WHERE table_id = ? ORDER BY column_name",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_column_semantic(
        &self,
        table_id: &str,
        column_name: &str,
        role: &str,
        is_kpi: bool,
        polarity: &str,
        label: Option<&str>,
        description: Option<&str>,
    ) -> StoreResult<ColumnSemanticRow> {
        let table_id = table_id.to_owned();
        let column_name = column_name.to_owned();
        let role = role.to_owned();
        let polarity = polarity.to_owned();
        let label = label.map(ToOwned::to_owned);
        let description = description.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<ColumnSemanticRow, _>(
                    conn,
                    r"INSERT INTO column_semantics (table_id, column_name, role, is_kpi, polarity, label, description)
                      VALUES (?, ?, ?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, column_name) DO UPDATE SET
                        role = excluded.role,
                        is_kpi = excluded.is_kpi,
                        polarity = excluded.polarity,
                        label = excluded.label,
                        description = excluded.description,
                        updated_at = datetime('now')
                      RETURNING *",
                    params![table_id, column_name, role, is_kpi, polarity, label, description],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn upsert_column_semantics_batch(
        &self,
        table_id: &str,
        rows: &[ColumnSemanticRow],
    ) -> StoreResult<()> {
        let table_id = table_id.to_owned();
        let rows = rows.to_vec();
        self.pool
            .transaction(move |tx| {
                let mut stmt = tx.prepare(
                    r"INSERT INTO column_semantics (table_id, column_name, role, is_kpi, polarity, label, description)
                      VALUES (?, ?, ?, ?, ?, ?, ?)
                      ON CONFLICT (table_id, column_name) DO UPDATE SET
                        role = excluded.role,
                        is_kpi = excluded.is_kpi,
                        polarity = excluded.polarity,
                        label = excluded.label,
                        description = excluded.description,
                        updated_at = datetime('now')",
                )?;
                for row in &rows {
                    stmt.execute(params![
                        table_id,
                        row.column_name,
                        row.role,
                        row.is_kpi,
                        row.polarity,
                        row.label,
                        row.description
                    ])?;
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_column_semantic(
        &self,
        table_id: &str,
        column_name: &str,
    ) -> StoreResult<bool> {
        let table_id = table_id.to_owned();
        let column_name = column_name.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM column_semantics WHERE table_id = ? AND column_name = ?",
                    params![table_id, column_name],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    pub async fn delete_all_column_semantics(&self, table_id: &str) -> StoreResult<u64> {
        let table_id = table_id.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM column_semantics WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(affected)
    }

    /// Check if any column_semantics rows exist for any table.
    pub async fn has_any_column_semantics(&self) -> StoreResult<bool> {
        let row = self
            .pool
            .call(|conn| fetch_one::<(i64,), _>(conn, "SELECT COUNT(*) FROM column_semantics", []))
            .await?;
        Ok(row.0 > 0)
    }

    // =====================================================
    // Table Analysis Settings CRUD
    // =====================================================

    pub async fn get_table_settings(
        &self,
        table_id: &str,
    ) -> StoreResult<Option<TableAnalysisSettingsRow>> {
        let table_id = table_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<TableAnalysisSettingsRow, _>(
                    conn,
                    "SELECT * FROM table_analysis_settings WHERE table_id = ?",
                    params![table_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn upsert_table_settings(
        &self,
        table_id: &str,
        display_name: Option<&str>,
        description: Option<&str>,
        time_granularity: Option<&str>,
        comparison_periods: Option<i32>,
    ) -> StoreResult<TableAnalysisSettingsRow> {
        let table_id = table_id.to_owned();
        let display_name = display_name.map(ToOwned::to_owned);
        let description = description.map(ToOwned::to_owned);
        let time_granularity = time_granularity.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<TableAnalysisSettingsRow, _>(
                    conn,
                    r"INSERT INTO table_analysis_settings (table_id, display_name, description, time_granularity, comparison_periods)
                      VALUES (?, ?, ?, ?, ?)
                      ON CONFLICT (table_id) DO UPDATE SET
                        display_name = excluded.display_name,
                        description = excluded.description,
                        time_granularity = excluded.time_granularity,
                        comparison_periods = excluded.comparison_periods,
                        updated_at = datetime('now')
                      RETURNING *",
                    params![
                        table_id,
                        display_name,
                        description,
                        time_granularity,
                        comparison_periods
                    ],
                )
            })
            .await?;
        Ok(row)
    }

    // =====================================================
    // Registered sources (connector-less: CSV uploads)
    // =====================================================

    pub async fn register_source(
        &self,
        source_id: &str,
        kind: &str,
        name: &str,
        meta_json: Option<&str>,
    ) -> StoreResult<SourceRow> {
        let source_id = source_id.to_owned();
        let kind = kind.to_owned();
        let name = name.to_owned();
        let meta_json = meta_json.map(ToOwned::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<SourceRow, _>(
                    conn,
                    r"INSERT INTO sources (source_id, kind, name, meta_json)
                      VALUES (?, ?, ?, ?)
                      ON CONFLICT (source_id) DO UPDATE SET
                        name = excluded.name,
                        meta_json = excluded.meta_json
                      RETURNING *",
                    params![source_id, kind, name, meta_json],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_registered_source(&self, source_id: &str) -> StoreResult<Option<SourceRow>> {
        let source_id = source_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SourceRow, _>(
                    conn,
                    "SELECT * FROM sources WHERE source_id = ?",
                    params![source_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_registered_sources(&self, kind: &str) -> StoreResult<Vec<SourceRow>> {
        let kind = kind.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<SourceRow, _>(
                    conn,
                    "SELECT * FROM sources WHERE kind = ? ORDER BY created_at DESC",
                    params![kind],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn delete_registered_source(&self, source_id: &str) -> StoreResult<bool> {
        let source_id = source_id.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM sources WHERE source_id = ?",
                    params![source_id],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    // =====================================================
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Fresh, fully migrated `StoreDb` backed by a SQLite file in `tmp`.
    async fn temp_db(tmp: &TempDir) -> StoreDb {
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        StoreDb::new(&db_url).await.expect("failed to open db")
    }

    /// Values must come deduplicated (many files share a partition value),
    /// sorted, and scoped to the requested table and key.
    #[tokio::test]
    async fn list_partition_values_dedupes_sorts_and_scopes() {
        let tmp = TempDir::new().expect("temp dir");
        let db = temp_db(&tmp).await;

        let table = db.create_table("events", "web:s1").await.expect("table");
        let other = db.create_table("events", "web:s2").await.expect("other");

        // Two files in date=2026-01-02, one in date=2026-01-01, plus a
        // different key and a different table that must both be excluded.
        for (path, date) in [
            ("f1.parquet", "2026-01-02"),
            ("f2.parquet", "2026-01-02"),
            ("f3.parquet", "2026-01-01"),
        ] {
            let file = db
                .add_table_file(&table.id, path, 1, 1)
                .await
                .expect("file");
            db.add_file_partitions(&file.id, &[("date", date)])
                .await
                .expect("partitions");
        }
        let hour_file = db
            .add_table_file(&table.id, "f4.parquet", 1, 1)
            .await
            .expect("file");
        db.add_file_partitions(&hour_file.id, &[("hour", "07")])
            .await
            .expect("partitions");
        let other_file = db
            .add_table_file(&other.id, "g1.parquet", 1, 1)
            .await
            .expect("file");
        db.add_file_partitions(&other_file.id, &[("date", "2025-12-31")])
            .await
            .expect("partitions");

        let values = db
            .list_partition_values(&table.id, "date")
            .await
            .expect("list");
        assert_eq!(values, vec!["2026-01-01", "2026-01-02"]);

        // Unknown key and empty table return empty, not an error.
        assert!(db
            .list_partition_values(&table.id, "region")
            .await
            .expect("list")
            .is_empty());
    }
}

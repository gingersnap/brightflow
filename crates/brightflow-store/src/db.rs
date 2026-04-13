//! SQLite-backed metadata store (Litehouse)

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::StoreResult;
use crate::models::{
    ColumnSemanticRow, ColumnStatRow, FileColumnStatRow, TableAnalysisSettingsRow, TableFileRow,
    TableRow,
};
use crate::scan::ScanFilter;

#[derive(Clone)]
pub struct StoreDb {
    pool: SqlitePool,
}

impl StoreDb {
    pub async fn new(database_url: &str) -> StoreResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true)
            .pragma("synchronous", "NORMAL")
            .pragma("cache_size", "-64000")
            .pragma("mmap_size", "268435456")
            .pragma("temp_store", "MEMORY")
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    // =====================================================
    // Tables CRUD
    // =====================================================

    pub async fn create_table(&self, name: &str) -> StoreResult<TableRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, TableRow>(
            r"INSERT INTO tables (id, name)
              VALUES (?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(name)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_table_by_name(&self, name: &str) -> StoreResult<Option<TableRow>> {
        let row = sqlx::query_as::<_, TableRow>("SELECT * FROM tables WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_tables(&self) -> StoreResult<Vec<TableRow>> {
        let rows = sqlx::query_as::<_, TableRow>("SELECT * FROM tables ORDER BY name")
            .fetch_all(&self.pool)
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
        let row = sqlx::query_as::<_, TableRow>(
            r"UPDATE tables
              SET schema_json = COALESCE(?, schema_json),
                  primary_keys = COALESCE(?, primary_keys),
                  total_rows = ?,
                  version = version + 1,
                  updated_at = datetime('now')
              WHERE id = ?
              RETURNING *",
        )
        .bind(schema_json)
        .bind(primary_keys)
        .bind(total_rows)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_table(&self, name: &str) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM tables WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
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
        let row = sqlx::query_as::<_, TableFileRow>(
            r"INSERT INTO table_files (id, table_id, path, num_rows, size_bytes)
              VALUES (?, ?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(table_id)
        .bind(path)
        .bind(num_rows)
        .bind(size_bytes)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_table_files(&self, table_id: &str) -> StoreResult<Vec<TableFileRow>> {
        let rows = sqlx::query_as::<_, TableFileRow>(
            "SELECT * FROM table_files WHERE table_id = ? ORDER BY added_at",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
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
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM table_files WHERE table_id = ?")
            .bind(table_id)
            .execute(&mut *tx)
            .await?;

        for (path, num_rows, size_bytes) in new_files {
            let id = uuid::Uuid::now_v7().to_string();
            sqlx::query(
                r"INSERT INTO table_files (id, table_id, path, num_rows, size_bytes)
                  VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(table_id)
            .bind(path)
            .bind(num_rows)
            .bind(size_bytes)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.list_table_files(table_id).await
    }

    pub async fn delete_table_files(&self, table_id: &str) -> StoreResult<u64> {
        let result = sqlx::query("DELETE FROM table_files WHERE table_id = ?")
            .bind(table_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    // =====================================================
    // Column Stats
    // =====================================================

    pub async fn upsert_column_stats(
        &self,
        table_id: &str,
        stats: &[ColumnStatRow],
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;

        // Clear existing stats for this table before inserting new ones
        sqlx::query("DELETE FROM table_column_stats WHERE table_id = ?")
            .bind(table_id)
            .execute(&mut *tx)
            .await?;

        for stat in stats {
            sqlx::query(
                r"INSERT INTO table_column_stats (table_id, column_name, min_value, max_value, null_count)
                  VALUES (?, ?, ?, ?, ?)",
            )
            .bind(table_id)
            .bind(&stat.column_name)
            .bind(&stat.min_value)
            .bind(&stat.max_value)
            .bind(stat.null_count)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_column_stats(&self, table_id: &str) -> StoreResult<Vec<ColumnStatRow>> {
        let rows = sqlx::query_as::<_, ColumnStatRow>(
            "SELECT * FROM table_column_stats WHERE table_id = ? ORDER BY column_name",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
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
    ) -> StoreResult<TableRow> {
        if let Some(row) = self.get_table_by_name(name).await? {
            return Ok(row);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, TableRow>(
            r"INSERT INTO tables (id, name, partition_columns)
              VALUES (?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(name)
        .bind(partition_columns)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Batch insert partition key/value pairs for a file.
    pub async fn add_file_partitions(
        &self,
        file_id: &str,
        partitions: &[(&str, &str)],
    ) -> StoreResult<()> {
        for (key, value) in partitions {
            sqlx::query(
                r"INSERT OR IGNORE INTO file_partitions (file_id, partition_key, partition_value)
                  VALUES (?, ?, ?)",
            )
            .bind(file_id)
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Batch insert per-file column statistics.
    pub async fn add_file_column_stats(&self, stats: &[FileColumnStatRow]) -> StoreResult<()> {
        for stat in stats {
            sqlx::query(
                r"INSERT OR IGNORE INTO file_column_stats (file_id, column_name, min_value, max_value, null_count)
                  VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&stat.file_id)
            .bind(&stat.column_name)
            .bind(&stat.min_value)
            .bind(&stat.max_value)
            .bind(stat.null_count)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Check if a file path is already registered for a table.
    pub async fn is_file_registered(&self, table_id: &str, path: &str) -> StoreResult<bool> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT COUNT(*) FROM table_files WHERE table_id = ? AND path = ?")
                .bind(table_id)
                .bind(path)
                .fetch_optional(&self.pool)
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
        let mut query = sqlx::query_as::<_, TableFileRow>(&sql);
        for val in &bind_values {
            query = query.bind(val);
        }
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows)
    }

    /// Get file IDs in a specific partition (for compaction).
    pub async fn get_partition_file_ids(
        &self,
        table_id: &str,
        key: &str,
        value: &str,
    ) -> StoreResult<Vec<TableFileRow>> {
        let rows = sqlx::query_as::<_, TableFileRow>(
            r"SELECT tf.id, tf.table_id, tf.path, tf.num_rows, tf.size_bytes, tf.added_at
              FROM table_files tf
              INNER JOIN file_partitions fp ON fp.file_id = tf.id
              WHERE tf.table_id = ?
                AND fp.partition_key = ?
                AND fp.partition_value = ?
              ORDER BY tf.added_at",
        )
        .bind(table_id)
        .bind(key)
        .bind(value)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Delete specific file records by ID (cascade cleans up partitions + stats).
    pub async fn delete_files_by_ids(&self, file_ids: &[String]) -> StoreResult<()> {
        for id in file_ids {
            sqlx::query("DELETE FROM table_files WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    // =====================================================
    // Column Semantics CRUD
    // =====================================================

    pub async fn get_column_semantics(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<ColumnSemanticRow>> {
        let rows = sqlx::query_as::<_, ColumnSemanticRow>(
            "SELECT * FROM column_semantics WHERE table_id = ? ORDER BY column_name",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn upsert_column_semantic(
        &self,
        table_id: &str,
        column_name: &str,
        role: &str,
        is_kpi: bool,
        label: Option<&str>,
        description: Option<&str>,
    ) -> StoreResult<ColumnSemanticRow> {
        let row = sqlx::query_as::<_, ColumnSemanticRow>(
            r"INSERT INTO column_semantics (table_id, column_name, role, is_kpi, label, description)
              VALUES (?, ?, ?, ?, ?, ?)
              ON CONFLICT (table_id, column_name) DO UPDATE SET
                role = excluded.role,
                is_kpi = excluded.is_kpi,
                label = excluded.label,
                description = excluded.description,
                updated_at = datetime('now')
              RETURNING *",
        )
        .bind(table_id)
        .bind(column_name)
        .bind(role)
        .bind(is_kpi)
        .bind(label)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn upsert_column_semantics_batch(
        &self,
        table_id: &str,
        rows: &[ColumnSemanticRow],
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;

        for row in rows {
            sqlx::query(
                r"INSERT INTO column_semantics (table_id, column_name, role, is_kpi, label, description)
                  VALUES (?, ?, ?, ?, ?, ?)
                  ON CONFLICT (table_id, column_name) DO UPDATE SET
                    role = excluded.role,
                    is_kpi = excluded.is_kpi,
                    label = excluded.label,
                    description = excluded.description,
                    updated_at = datetime('now')",
            )
            .bind(table_id)
            .bind(&row.column_name)
            .bind(&row.role)
            .bind(row.is_kpi)
            .bind(&row.label)
            .bind(&row.description)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_column_semantic(
        &self,
        table_id: &str,
        column_name: &str,
    ) -> StoreResult<bool> {
        let result =
            sqlx::query("DELETE FROM column_semantics WHERE table_id = ? AND column_name = ?")
                .bind(table_id)
                .bind(column_name)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_all_column_semantics(&self, table_id: &str) -> StoreResult<u64> {
        let result = sqlx::query("DELETE FROM column_semantics WHERE table_id = ?")
            .bind(table_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Check if any column_semantics rows exist for any table.
    pub async fn has_any_column_semantics(&self) -> StoreResult<bool> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM column_semantics")
            .fetch_one(&self.pool)
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
        let row = sqlx::query_as::<_, TableAnalysisSettingsRow>(
            "SELECT * FROM table_analysis_settings WHERE table_id = ?",
        )
        .bind(table_id)
        .fetch_optional(&self.pool)
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
        let row = sqlx::query_as::<_, TableAnalysisSettingsRow>(
            r"INSERT INTO table_analysis_settings (table_id, display_name, description, time_granularity, comparison_periods)
              VALUES (?, ?, ?, ?, ?)
              ON CONFLICT (table_id) DO UPDATE SET
                display_name = excluded.display_name,
                description = excluded.description,
                time_granularity = excluded.time_granularity,
                comparison_periods = excluded.comparison_periods,
                updated_at = datetime('now')
              RETURNING *",
        )
        .bind(table_id)
        .bind(display_name)
        .bind(description)
        .bind(time_granularity)
        .bind(comparison_periods)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_table_settings(&self, table_id: &str) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM table_analysis_settings WHERE table_id = ?")
            .bind(table_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

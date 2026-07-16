//! SQLite-backed metadata store (Litehouse)

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::{StoreError, StoreResult};
use crate::models::{
    ActionLogRow, AgentRunRow, ClusterEditRow, ColumnSemanticRow, ColumnStatRow, DocumentLabelRow,
    DocumentLabelWithName, ExcludedTermRow, FileColumnStatRow, InsightHistoryRow, InsightStateRow,
    InsightSuppressionRow, TableAnalysisSettingsRow, TableEnrichmentSettingsRow, TableFileRow,
    TableRow, TaxonomyCategoryRow,
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

    pub async fn create_table(&self, name: &str, source_id: &str) -> StoreResult<TableRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, TableRow>(
            r"INSERT INTO tables (id, name, source_id)
              VALUES (?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(name)
        .bind(source_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_table(&self, source_id: &str, name: &str) -> StoreResult<Option<TableRow>> {
        let row =
            sqlx::query_as::<_, TableRow>("SELECT * FROM tables WHERE source_id = ? AND name = ?")
                .bind(source_id)
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

    pub async fn list_tables_by_source(&self, source_id: &str) -> StoreResult<Vec<TableRow>> {
        let rows =
            sqlx::query_as::<_, TableRow>("SELECT * FROM tables WHERE source_id = ? ORDER BY name")
                .bind(source_id)
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

    pub async fn delete_table(&self, source_id: &str, name: &str) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM tables WHERE source_id = ? AND name = ?")
            .bind(source_id)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete all tables belonging to a source. Cascades to table_files,
    /// column_stats, file_partitions, file_column_stats, column_semantics,
    /// and table_analysis_settings via `ON DELETE CASCADE`.
    pub async fn delete_tables_by_source(&self, source_id: &str) -> StoreResult<u64> {
        let result = sqlx::query("DELETE FROM tables WHERE source_id = ?")
            .bind(source_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
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
        source_id: &str,
    ) -> StoreResult<TableRow> {
        if let Some(row) = self.get_table(source_id, name).await? {
            return Ok(row);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, TableRow>(
            r"INSERT INTO tables (id, name, partition_columns, source_id)
              VALUES (?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(name)
        .bind(partition_columns)
        .bind(source_id)
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

    // =====================================================
    // Table Enrichment Settings CRUD
    // =====================================================

    pub async fn get_enrichment_settings(
        &self,
        table_id: &str,
    ) -> StoreResult<Option<TableEnrichmentSettingsRow>> {
        let row = sqlx::query_as::<_, TableEnrichmentSettingsRow>(
            "SELECT * FROM table_enrichment_settings WHERE table_id = ?",
        )
        .bind(table_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_all_enrichment_settings(
        &self,
    ) -> StoreResult<Vec<TableEnrichmentSettingsRow>> {
        let rows = sqlx::query_as::<_, TableEnrichmentSettingsRow>(
            "SELECT * FROM table_enrichment_settings",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_enrichment_settings(
        &self,
        table_id: &str,
        text_columns: Option<&str>,
        cleaning_profile: Option<&str>,
        language_column: Option<&str>,
        embedder: Option<&str>,
        min_cluster_size: Option<i64>,
        algorithm: Option<&str>,
    ) -> StoreResult<TableEnrichmentSettingsRow> {
        let row = sqlx::query_as::<_, TableEnrichmentSettingsRow>(
            r"INSERT INTO table_enrichment_settings
                (table_id, text_columns, cleaning_profile, language_column, embedder, min_cluster_size, algorithm)
              VALUES (?, ?, ?, ?, ?, ?, ?)
              ON CONFLICT (table_id) DO UPDATE SET
                text_columns = excluded.text_columns,
                cleaning_profile = excluded.cleaning_profile,
                language_column = excluded.language_column,
                embedder = excluded.embedder,
                min_cluster_size = excluded.min_cluster_size,
                algorithm = excluded.algorithm,
                updated_at = datetime('now')
              RETURNING *",
        )
        .bind(table_id)
        .bind(text_columns)
        .bind(cleaning_profile)
        .bind(language_column)
        .bind(embedder)
        .bind(min_cluster_size)
        .bind(algorithm)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_enrichment_settings(&self, table_id: &str) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM table_enrichment_settings WHERE table_id = ?")
            .bind(table_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
    // Insight History CRUD (novelty decay input)
    // =====================================================

    pub async fn get_insight_history(&self, table_id: &str) -> StoreResult<Vec<InsightHistoryRow>> {
        let rows = sqlx::query_as::<_, InsightHistoryRow>(
            "SELECT * FROM insight_history WHERE table_id = ?",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Record that a batch of insights was shown: bumps shown_count and
    /// last_shown_at; a changed value signature resets the count (the story
    /// developed, so it is novel again).
    pub async fn record_shown_insights(
        &self,
        table_id: &str,
        shown: &[(String, String, String, String)], // (fingerprint, identity, insight_type, value_sig)
        now_epoch: i64,
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        for (fingerprint, identity, insight_type, value_sig) in shown {
            sqlx::query(
                r"INSERT INTO insight_history
                    (table_id, fingerprint, identity, insight_type, last_value_sig,
                     shown_count, first_shown_at, last_shown_at)
                  VALUES (?, ?, ?, ?, ?, 1, ?, ?)
                  ON CONFLICT (table_id, fingerprint) DO UPDATE SET
                    identity = excluded.identity,
                    insight_type = excluded.insight_type,
                    shown_count = CASE
                        WHEN insight_history.last_value_sig != excluded.last_value_sig THEN 1
                        ELSE insight_history.shown_count + 1
                    END,
                    last_value_sig = excluded.last_value_sig,
                    last_shown_at = excluded.last_shown_at",
            )
            .bind(table_id)
            .bind(fingerprint)
            .bind(identity)
            .bind(insight_type)
            .bind(value_sig)
            .bind(now_epoch)
            .bind(now_epoch)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn reset_insight_history(&self, table_id: &str) -> StoreResult<u64> {
        let result = sqlx::query("DELETE FROM insight_history WHERE table_id = ?")
            .bind(table_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    // =====================================================
    // Insight State (dismiss / pin) + Suppressions
    // =====================================================

    pub async fn get_insight_states(&self, table_id: &str) -> StoreResult<Vec<InsightStateRow>> {
        let rows =
            sqlx::query_as::<_, InsightStateRow>("SELECT * FROM insight_state WHERE table_id = ?")
                .bind(table_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    pub async fn upsert_insight_state(
        &self,
        table_id: &str,
        fingerprint: &str,
        state: &str,
        reason: Option<&str>,
        annotation: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<InsightStateRow> {
        let row = sqlx::query_as::<_, InsightStateRow>(
            r"INSERT INTO insight_state (table_id, fingerprint, state, reason, annotation, created_at)
              VALUES (?, ?, ?, ?, ?, ?)
              ON CONFLICT (table_id, fingerprint) DO UPDATE SET
                state = excluded.state,
                reason = excluded.reason,
                annotation = excluded.annotation,
                created_at = excluded.created_at
              RETURNING *",
        )
        .bind(table_id)
        .bind(fingerprint)
        .bind(state)
        .bind(reason)
        .bind(annotation)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_insight_state(
        &self,
        table_id: &str,
        fingerprint: &str,
    ) -> StoreResult<bool> {
        let result =
            sqlx::query("DELETE FROM insight_state WHERE table_id = ? AND fingerprint = ?")
                .bind(table_id)
                .bind(fingerprint)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_insight_suppressions(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<InsightSuppressionRow>> {
        let rows = sqlx::query_as::<_, InsightSuppressionRow>(
            "SELECT * FROM insight_suppressions WHERE table_id = ?",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add_insight_suppression(
        &self,
        table_id: &str,
        kind: &str,
        target: &str,
        now_epoch: i64,
    ) -> StoreResult<InsightSuppressionRow> {
        let row = sqlx::query_as::<_, InsightSuppressionRow>(
            r"INSERT INTO insight_suppressions (table_id, kind, target, created_at)
              VALUES (?, ?, ?, ?)
              ON CONFLICT (table_id, kind, target) DO UPDATE SET created_at = excluded.created_at
              RETURNING *",
        )
        .bind(table_id)
        .bind(kind)
        .bind(target)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_insight_suppression(&self, id: i64) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM insight_suppressions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
    // Action log
    // =====================================================

    /// Insert a new action-log entry. Returns None when the request_id was
    /// already recorded (idempotent replay — fetch the existing row instead).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_action(
        &self,
        request_id: &str,
        actor_type: &str,
        agent_run_id: Option<i64>,
        action_kind: &str,
        params_json: &str,
        status: &str,
        now_epoch: i64,
    ) -> StoreResult<Option<ActionLogRow>> {
        let row = sqlx::query_as::<_, ActionLogRow>(
            r"INSERT INTO action_log
                (request_id, actor_type, agent_run_id, action_kind, params_json, status, created_at)
              VALUES (?, ?, ?, ?, ?, ?, ?)
              ON CONFLICT (request_id) DO NOTHING
              RETURNING *",
        )
        .bind(request_id)
        .bind(actor_type)
        .bind(agent_run_id)
        .bind(action_kind)
        .bind(params_json)
        .bind(status)
        .bind(now_epoch)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_action_by_request_id(
        &self,
        request_id: &str,
    ) -> StoreResult<Option<ActionLogRow>> {
        let row =
            sqlx::query_as::<_, ActionLogRow>("SELECT * FROM action_log WHERE request_id = ?")
                .bind(request_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    pub async fn get_action(&self, id: i64) -> StoreResult<Option<ActionLogRow>> {
        let row = sqlx::query_as::<_, ActionLogRow>("SELECT * FROM action_log WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_actions(&self, limit: i64) -> StoreResult<Vec<ActionLogRow>> {
        let rows = sqlx::query_as::<_, ActionLogRow>(
            "SELECT * FROM action_log ORDER BY created_at DESC, id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Every action awaiting approval, **oldest first**.
    ///
    /// Ascending id is load-bearing, not cosmetic: proposals have ordering
    /// dependencies. `propose_taxonomy` defines a category and `label_documents`
    /// then references it by name, so approving the label before the category
    /// exists fails with "not a category in this table's taxonomy". Creation
    /// order is the order they were meant to apply in.
    pub async fn list_proposed_actions(&self) -> StoreResult<Vec<ActionLogRow>> {
        let rows = sqlx::query_as::<_, ActionLogRow>(
            "SELECT * FROM action_log WHERE status = 'proposed' ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Count of actions awaiting approval. Cheap enough to poll for a badge.
    pub async fn count_proposed_actions(&self) -> StoreResult<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM action_log WHERE status = 'proposed'")
                .fetch_one(&self.pool)
                .await?;
        Ok(n)
    }

    pub async fn list_actions_for_agent_run(&self, run_id: i64) -> StoreResult<Vec<ActionLogRow>> {
        let rows = sqlx::query_as::<_, ActionLogRow>(
            "SELECT * FROM action_log WHERE agent_run_id = ? ORDER BY id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn update_action_result(
        &self,
        id: i64,
        status: &str,
        result_json: Option<&str>,
        undo_json: Option<&str>,
        resolved_at: i64,
    ) -> StoreResult<()> {
        sqlx::query(
            r"UPDATE action_log
              SET status = ?, result_json = ?, undo_json = ?, resolved_at = ?
              WHERE id = ?",
        )
        .bind(status)
        .bind(result_json)
        .bind(undo_json)
        .bind(resolved_at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_action_status(
        &self,
        id: i64,
        status: &str,
        resolved_at: i64,
    ) -> StoreResult<()> {
        sqlx::query("UPDATE action_log SET status = ?, resolved_at = ? WHERE id = ?")
            .bind(status)
            .bind(resolved_at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // =====================================================
    // Cluster edits (curation overlay)
    // =====================================================

    pub async fn get_cluster_edits(&self, table_id: &str) -> StoreResult<Vec<ClusterEditRow>> {
        let rows =
            sqlx::query_as::<_, ClusterEditRow>("SELECT * FROM cluster_edits WHERE table_id = ?")
                .bind(table_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    /// Upsert an edit keyed by (table, centroid fingerprint). `Some(inner)`
    /// fields overwrite; `None` fields keep the existing value.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_cluster_edit(
        &self,
        table_id: &str,
        centroid_fingerprint: &str,
        centroid_json: &str,
        cluster_id: Option<i64>,
        custom_name: Option<Option<&str>>,
        label: Option<Option<&str>>,
        is_noise: Option<bool>,
        merged_into: Option<Option<i64>>,
        now_epoch: i64,
    ) -> StoreResult<ClusterEditRow> {
        sqlx::query(
            r"INSERT INTO cluster_edits (table_id, centroid_fingerprint, centroid_json, cluster_id, updated_at)
              VALUES (?, ?, ?, ?, ?)
              ON CONFLICT (table_id, centroid_fingerprint) DO UPDATE SET
                centroid_json = excluded.centroid_json,
                cluster_id = excluded.cluster_id,
                orphaned = 0,
                updated_at = excluded.updated_at",
        )
        .bind(table_id)
        .bind(centroid_fingerprint)
        .bind(centroid_json)
        .bind(cluster_id)
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;

        if let Some(v) = custom_name {
            sqlx::query("UPDATE cluster_edits SET custom_name = ? WHERE table_id = ? AND centroid_fingerprint = ?")
                .bind(v).bind(table_id).bind(centroid_fingerprint).execute(&self.pool).await?;
        }
        if let Some(v) = label {
            sqlx::query("UPDATE cluster_edits SET label = ? WHERE table_id = ? AND centroid_fingerprint = ?")
                .bind(v).bind(table_id).bind(centroid_fingerprint).execute(&self.pool).await?;
        }
        if let Some(v) = is_noise {
            sqlx::query("UPDATE cluster_edits SET is_noise = ? WHERE table_id = ? AND centroid_fingerprint = ?")
                .bind(v).bind(table_id).bind(centroid_fingerprint).execute(&self.pool).await?;
        }
        if let Some(v) = merged_into {
            sqlx::query("UPDATE cluster_edits SET merged_into = ? WHERE table_id = ? AND centroid_fingerprint = ?")
                .bind(v).bind(table_id).bind(centroid_fingerprint).execute(&self.pool).await?;
        }

        let row = sqlx::query_as::<_, ClusterEditRow>(
            "SELECT * FROM cluster_edits WHERE table_id = ? AND centroid_fingerprint = ?",
        )
        .bind(table_id)
        .bind(centroid_fingerprint)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Reconciliation write-back: point an edit at a new raw cluster id (or
    /// orphan it when no new centroid matches).
    pub async fn set_cluster_edit_target(
        &self,
        edit_id: i64,
        cluster_id: Option<i64>,
        centroid_json: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<()> {
        if let Some(json) = centroid_json {
            sqlx::query(
                r"UPDATE cluster_edits
                  SET cluster_id = ?, centroid_json = ?, orphaned = 0, updated_at = ?
                  WHERE id = ?",
            )
            .bind(cluster_id)
            .bind(json)
            .bind(now_epoch)
            .bind(edit_id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                r"UPDATE cluster_edits
                  SET cluster_id = NULL, orphaned = 1, updated_at = ?
                  WHERE id = ?",
            )
            .bind(now_epoch)
            .bind(edit_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn delete_cluster_edit(&self, edit_id: i64) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM cluster_edits WHERE id = ?")
            .bind(edit_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
    // Excluded terms
    // =====================================================

    pub async fn get_excluded_terms(&self, table_id: &str) -> StoreResult<Vec<ExcludedTermRow>> {
        let rows = sqlx::query_as::<_, ExcludedTermRow>(
            "SELECT * FROM excluded_terms WHERE table_id = ? ORDER BY term",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn add_excluded_term(
        &self,
        table_id: &str,
        term: &str,
        now_epoch: i64,
    ) -> StoreResult<ExcludedTermRow> {
        let row = sqlx::query_as::<_, ExcludedTermRow>(
            r"INSERT INTO excluded_terms (table_id, term, created_at)
              VALUES (?, ?, ?)
              ON CONFLICT (table_id, term) DO UPDATE SET created_at = excluded.created_at
              RETURNING *",
        )
        .bind(table_id)
        .bind(term)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn remove_excluded_term(&self, table_id: &str, term: &str) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM excluded_terms WHERE table_id = ? AND term = ?")
            .bind(table_id)
            .bind(term)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
    // Intent taxonomy
    // =====================================================

    pub async fn get_taxonomy_categories(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<TaxonomyCategoryRow>> {
        let rows = sqlx::query_as::<_, TaxonomyCategoryRow>(
            "SELECT * FROM taxonomy_categories WHERE table_id = ? ORDER BY name",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Define (or touch) an intent category. Idempotent on (table, name).
    pub async fn upsert_taxonomy_category(
        &self,
        table_id: &str,
        name: &str,
        description: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<TaxonomyCategoryRow> {
        let row = sqlx::query_as::<_, TaxonomyCategoryRow>(
            r"INSERT INTO taxonomy_categories (table_id, name, description, created_at)
              VALUES (?, ?, ?, ?)
              ON CONFLICT (table_id, name) DO UPDATE SET
                description = COALESCE(excluded.description, taxonomy_categories.description)
              RETURNING *",
        )
        .bind(table_id)
        .bind(name)
        .bind(description)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_taxonomy_category(
        &self,
        category_id: i64,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let row = sqlx::query_as::<_, TaxonomyCategoryRow>(
            "SELECT * FROM taxonomy_categories WHERE id = ?",
        )
        .bind(category_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_taxonomy_category_by_name(
        &self,
        table_id: &str,
        name: &str,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let row = sqlx::query_as::<_, TaxonomyCategoryRow>(
            "SELECT * FROM taxonomy_categories WHERE table_id = ? AND name = ?",
        )
        .bind(table_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Rename a category. The human's right to fix the LLM's wording.
    pub async fn rename_taxonomy_category(
        &self,
        category_id: i64,
        name: &str,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let row = sqlx::query_as::<_, TaxonomyCategoryRow>(
            "UPDATE taxonomy_categories SET name = ? WHERE id = ? RETURNING *",
        )
        .bind(name)
        .bind(category_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Restore a category's name and description (undo path).
    pub async fn update_taxonomy_category(
        &self,
        category_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> StoreResult<Option<TaxonomyCategoryRow>> {
        let row = sqlx::query_as::<_, TaxonomyCategoryRow>(
            "UPDATE taxonomy_categories SET name = ?, description = ? WHERE id = ? RETURNING *",
        )
        .bind(name)
        .bind(description)
        .bind(category_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Delete a category. `document_labels` cascade via the FK
    /// (`foreign_keys(true)` is set on the pool, so the cascade really fires).
    pub async fn delete_taxonomy_category(&self, category_id: i64) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM taxonomy_categories WHERE id = ?")
            .bind(category_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Recreate a deleted category under its ORIGINAL id, along with the row
    /// labels that cascaded away (undo path).
    ///
    /// The explicit id is load-bearing: labels reference categories by id, so
    /// reinserting under a fresh autoincrement id would restore the category
    /// but silently orphan every label that pointed at it.
    pub async fn recreate_taxonomy_category(
        &self,
        category_id: i64,
        table_id: &str,
        name: &str,
        description: Option<&str>,
        created_at: i64,
        labels: &[(String, String, i64)],
    ) -> StoreResult<()> {
        // The table carries TWO uniqueness constraints — the primary key AND
        // UNIQUE(table_id, name) — so `ON CONFLICT (id)` alone does not make
        // this insert safe. If the name was re-defined under a NEW id after the
        // delete, reinserting the old row violates the name constraint, the
        // transaction rolls back, and the snapshotted labels are unrecoverable.
        // Detect that case and say so, instead of surfacing a raw 500.
        if let Some(existing) = self.get_taxonomy_category_by_name(table_id, name).await? {
            if existing.id != category_id {
                return Err(StoreError::Other(format!(
                    "cannot restore category '{name}': a different category (id {}) now uses \
                     that name. Rename or remove it first, then retry the undo.",
                    existing.id
                )));
            }
        }

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r"INSERT INTO taxonomy_categories (id, table_id, name, description, created_at)
              VALUES (?, ?, ?, ?, ?)
              ON CONFLICT (id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description",
        )
        .bind(category_id)
        .bind(table_id)
        .bind(name)
        .bind(description)
        .bind(created_at)
        .execute(&mut *tx)
        .await?;

        for (row_id, source, label_created_at) in labels {
            sqlx::query(
                r"INSERT INTO document_labels (table_id, row_id, category_id, source, created_at)
                  VALUES (?, ?, ?, ?, ?)
                  ON CONFLICT (table_id, row_id, category_id) DO NOTHING",
            )
            .bind(table_id)
            .bind(row_id)
            .bind(category_id)
            .bind(source)
            .bind(label_created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =====================================================
    // Document labels (row-level supervision)
    // =====================================================

    /// Every labelled row for a table, with category names — the training feed.
    pub async fn get_document_labels(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<DocumentLabelWithName>> {
        let rows = sqlx::query_as::<_, DocumentLabelWithName>(
            r"SELECT dl.row_id, dl.category_id, tc.name, dl.source
              FROM document_labels dl
              JOIN taxonomy_categories tc ON tc.id = dl.category_id
              WHERE dl.table_id = ?
              ORDER BY dl.row_id, tc.name",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Labels for one row.
    pub async fn get_labels_for_row(
        &self,
        table_id: &str,
        row_id: &str,
    ) -> StoreResult<Vec<DocumentLabelWithName>> {
        let rows = sqlx::query_as::<_, DocumentLabelWithName>(
            r"SELECT dl.row_id, dl.category_id, tc.name, dl.source
              FROM document_labels dl
              JOIN taxonomy_categories tc ON tc.id = dl.category_id
              WHERE dl.table_id = ? AND dl.row_id = ?
              ORDER BY tc.name",
        )
        .bind(table_id)
        .bind(row_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Replace a row's entire label set, transactionally.
    ///
    /// Replace rather than merge: the curation UI shows a row's labels as a set
    /// and the human edits that set, so a partial write would leave labels the
    /// curator thought they had removed. The delete+insert runs in one
    /// transaction so a failure can't leave the row unlabelled.
    pub async fn set_document_labels(
        &self,
        table_id: &str,
        row_id: &str,
        category_ids: &[i64],
        source: &str,
        now_epoch: i64,
    ) -> StoreResult<Vec<DocumentLabelRow>> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM document_labels WHERE table_id = ? AND row_id = ?")
            .bind(table_id)
            .bind(row_id)
            .execute(&mut *tx)
            .await?;

        for category_id in category_ids {
            sqlx::query(
                r"INSERT INTO document_labels (table_id, row_id, category_id, source, created_at)
                  VALUES (?, ?, ?, ?, ?)
                  ON CONFLICT (table_id, row_id, category_id) DO UPDATE SET
                    source = excluded.source,
                    created_at = excluded.created_at",
            )
            .bind(table_id)
            .bind(row_id)
            .bind(category_id)
            .bind(source)
            .bind(now_epoch)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        let rows = sqlx::query_as::<_, DocumentLabelRow>(
            "SELECT * FROM document_labels WHERE table_id = ? AND row_id = ? ORDER BY category_id",
        )
        .bind(table_id)
        .bind(row_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Raw label rows for one document row (undo capture needs `source` and
    /// `created_at`, which the name-joined view drops).
    pub async fn get_label_rows_for_row(
        &self,
        table_id: &str,
        row_id: &str,
    ) -> StoreResult<Vec<DocumentLabelRow>> {
        let rows = sqlx::query_as::<_, DocumentLabelRow>(
            "SELECT * FROM document_labels WHERE table_id = ? AND row_id = ? ORDER BY category_id",
        )
        .bind(table_id)
        .bind(row_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Every label attached to a category — snapshotted before a delete so the
    /// cascade can be undone.
    pub async fn get_document_labels_for_category(
        &self,
        category_id: i64,
    ) -> StoreResult<Vec<DocumentLabelRow>> {
        let rows = sqlx::query_as::<_, DocumentLabelRow>(
            "SELECT * FROM document_labels WHERE category_id = ? ORDER BY row_id",
        )
        .bind(category_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Restore a row's exact prior label set, preserving each label's original
    /// `source` and `created_at` (undo path).
    ///
    /// An empty `labels` clears the row — that is a real prior state ("this row
    /// was unlabelled"), not a no-op.
    pub async fn restore_document_labels(
        &self,
        table_id: &str,
        row_id: &str,
        labels: &[(i64, String, i64)],
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM document_labels WHERE table_id = ? AND row_id = ?")
            .bind(table_id)
            .bind(row_id)
            .execute(&mut *tx)
            .await?;

        for (category_id, source, created_at) in labels {
            sqlx::query(
                r"INSERT INTO document_labels (table_id, row_id, category_id, source, created_at)
                  VALUES (?, ?, ?, ?, ?)
                  ON CONFLICT (table_id, row_id, category_id) DO UPDATE SET
                    source = excluded.source,
                    created_at = excluded.created_at",
            )
            .bind(table_id)
            .bind(row_id)
            .bind(category_id)
            .bind(source)
            .bind(created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Per-category labelled-row counts — drives `MIN_LABEL_SUPPORT` feedback in
    /// the curation UI ("this category has too few examples to train on").
    pub async fn count_labels_per_category(&self, table_id: &str) -> StoreResult<Vec<(i64, i64)>> {
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            r"SELECT category_id, COUNT(*) as n
              FROM document_labels WHERE table_id = ?
              GROUP BY category_id",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Distinct rows carrying at least one label.
    pub async fn count_labelled_rows(&self, table_id: &str) -> StoreResult<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT row_id) FROM document_labels WHERE table_id = ?")
                .bind(table_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(n)
    }

    // =====================================================
    // Agent runs
    // =====================================================

    pub async fn insert_agent_run(
        &self,
        kind: &str,
        mode: &str,
        scope: &str,
        now_epoch: i64,
    ) -> StoreResult<AgentRunRow> {
        let row = sqlx::query_as::<_, AgentRunRow>(
            r"INSERT INTO agent_runs (kind, mode, scope, status, created_at)
              VALUES (?, ?, ?, 'running', ?)
              RETURNING *",
        )
        .bind(kind)
        .bind(mode)
        .bind(scope)
        .bind(now_epoch)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_agent_run(&self, id: i64) -> StoreResult<Option<AgentRunRow>> {
        let row = sqlx::query_as::<_, AgentRunRow>("SELECT * FROM agent_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_agent_runs(&self, limit: i64) -> StoreResult<Vec<AgentRunRow>> {
        let rows = sqlx::query_as::<_, AgentRunRow>(
            "SELECT * FROM agent_runs ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn active_agent_run_for_scope(
        &self,
        scope: &str,
    ) -> StoreResult<Option<AgentRunRow>> {
        let row = sqlx::query_as::<_, AgentRunRow>(
            "SELECT * FROM agent_runs WHERE scope = ? AND status = 'running' LIMIT 1",
        )
        .bind(scope)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn finish_agent_run(
        &self,
        id: i64,
        status: &str,
        detail: Option<&str>,
        now_epoch: i64,
    ) -> StoreResult<()> {
        sqlx::query("UPDATE agent_runs SET status = ?, detail = ?, finished_at = ? WHERE id = ?")
            .bind(status)
            .bind(detail)
            .bind(now_epoch)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Boot cleanup: any run still 'running' from a previous process crashed.
    pub async fn fail_stuck_agent_runs(&self, now_epoch: i64) -> StoreResult<u64> {
        let result = sqlx::query(
            r"UPDATE agent_runs SET status = 'failed', detail = 'process restarted mid-run',
              finished_at = ? WHERE status = 'running'",
        )
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

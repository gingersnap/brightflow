//! Enrichment persistence: functions with immutable versions, runs, the
//! LLM output cache, and the deprecated `table_enrichment_settings`
//! dual-write kept for one release (reads have moved to functions).

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::{
    EnrichmentCacheRow, EnrichmentFunctionRow, EnrichmentFunctionVersionRow, EnrichmentRunRow,
    TableEnrichmentSettingsRow,
};

impl StoreDb {
    // Table Enrichment Settings CRUD
    // =====================================================

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

    // =====================================================
    // Enrichment functions (header + immutable versions)
    // =====================================================

    /// Create a draft function with its version-1 config snapshot.
    pub async fn create_enrichment_function(
        &self,
        table_id: &str,
        name: &str,
        kind: &str,
        status: &str,
        config_json: &str,
    ) -> StoreResult<EnrichmentFunctionRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, EnrichmentFunctionRow>(
            r"INSERT INTO enrichment_functions (id, table_id, name, kind, status, current_version)
              VALUES (?, ?, ?, ?, ?, 1)
              RETURNING *",
        )
        .bind(&id)
        .bind(table_id)
        .bind(name)
        .bind(kind)
        .bind(status)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r"INSERT INTO enrichment_function_versions (function_id, version, config_json)
              VALUES (?, 1, ?)",
        )
        .bind(&id)
        .bind(config_json)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn get_enrichment_function(
        &self,
        id: &str,
    ) -> StoreResult<Option<EnrichmentFunctionRow>> {
        let row = sqlx::query_as::<_, EnrichmentFunctionRow>(
            "SELECT * FROM enrichment_functions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_enrichment_functions(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<EnrichmentFunctionRow>> {
        let rows = sqlx::query_as::<_, EnrichmentFunctionRow>(
            "SELECT * FROM enrichment_functions WHERE table_id = ? ORDER BY created_at",
        )
        .bind(table_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Promoted functions of one kind for a table — the sync-time gate.
    pub async fn list_promoted_functions(
        &self,
        table_id: &str,
        kind: &str,
    ) -> StoreResult<Vec<EnrichmentFunctionRow>> {
        let rows = sqlx::query_as::<_, EnrichmentFunctionRow>(
            r"SELECT * FROM enrichment_functions
              WHERE table_id = ? AND kind = ? AND status = 'promoted'
              ORDER BY created_at",
        )
        .bind(table_id)
        .bind(kind)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Append a new config version and bump the header. Returns the new
    /// version number.
    /// Current-version `config_json` of the table's promoted function of
    /// `kind`, or None when no promoted function exists. One join, shared by
    /// every consumer that resolves an effective enrichment config (API
    /// hydration, scheduler syncs, CLI) so they cannot disagree on what
    /// "promoted" means. Oldest promoted function wins, matching the
    /// `list_promoted_functions` + first() sites it replaces.
    pub async fn get_promoted_function_config(
        &self,
        table_id: &str,
        kind: &str,
    ) -> StoreResult<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            r"SELECT v.config_json
              FROM enrichment_functions f
              JOIN enrichment_function_versions v
                ON v.function_id = f.id AND v.version = f.current_version
              WHERE f.table_id = ? AND f.kind = ? AND f.status = 'promoted'
              ORDER BY f.created_at
              LIMIT 1",
        )
        .bind(table_id)
        .bind(kind)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(json,)| json))
    }

    pub async fn update_enrichment_function_config(
        &self,
        id: &str,
        config_json: &str,
    ) -> StoreResult<i64> {
        let mut tx = self.pool.begin().await?;
        let (next,): (i64,) =
            sqlx::query_as("SELECT current_version + 1 FROM enrichment_functions WHERE id = ?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        sqlx::query(
            r"INSERT INTO enrichment_function_versions (function_id, version, config_json)
              VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(next)
        .bind(config_json)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r"UPDATE enrichment_functions
              SET current_version = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(next)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(next)
    }

    pub async fn set_enrichment_function_status(
        &self,
        id: &str,
        status: &str,
    ) -> StoreResult<bool> {
        let result = sqlx::query(
            r"UPDATE enrichment_functions
              SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete a function; versions, runs and cache cascade away.
    pub async fn delete_enrichment_function(&self, id: &str) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM enrichment_functions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_enrichment_function_versions(
        &self,
        function_id: &str,
    ) -> StoreResult<Vec<EnrichmentFunctionVersionRow>> {
        let rows = sqlx::query_as::<_, EnrichmentFunctionVersionRow>(
            r"SELECT * FROM enrichment_function_versions
              WHERE function_id = ? ORDER BY version DESC",
        )
        .bind(function_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_enrichment_function_version(
        &self,
        function_id: &str,
        version: i64,
    ) -> StoreResult<Option<EnrichmentFunctionVersionRow>> {
        let row = sqlx::query_as::<_, EnrichmentFunctionVersionRow>(
            "SELECT * FROM enrichment_function_versions WHERE function_id = ? AND version = ?",
        )
        .bind(function_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // =====================================================
    // Enrichment runs
    // =====================================================

    pub async fn insert_enrichment_run(
        &self,
        function_id: &str,
        version: i64,
        mode: &str,
        rows_total: i64,
    ) -> StoreResult<EnrichmentRunRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let row = sqlx::query_as::<_, EnrichmentRunRow>(
            r"INSERT INTO enrichment_runs (id, function_id, version, mode, rows_total)
              VALUES (?, ?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(function_id)
        .bind(version)
        .bind(mode)
        .bind(rows_total)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn update_enrichment_run_progress(
        &self,
        id: &str,
        rows_done: i64,
        rows_failed: i64,
        rows_cached: i64,
        prompt_tokens: i64,
        completion_tokens: i64,
    ) -> StoreResult<()> {
        sqlx::query(
            r"UPDATE enrichment_runs
              SET rows_done = ?, rows_failed = ?, rows_cached = ?,
                  prompt_tokens = ?, completion_tokens = ?,
                  total_tokens = ? + ?
              WHERE id = ?",
        )
        .bind(rows_done)
        .bind(rows_failed)
        .bind(rows_cached)
        .bind(prompt_tokens)
        .bind(completion_tokens)
        .bind(prompt_tokens)
        .bind(completion_tokens)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn finish_enrichment_run(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> StoreResult<()> {
        sqlx::query(
            r"UPDATE enrichment_runs
              SET status = ?, error = ?, finished_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_enrichment_run(&self, id: &str) -> StoreResult<Option<EnrichmentRunRow>> {
        let row =
            sqlx::query_as::<_, EnrichmentRunRow>("SELECT * FROM enrichment_runs WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    pub async fn active_enrichment_run(
        &self,
        function_id: &str,
    ) -> StoreResult<Option<EnrichmentRunRow>> {
        let row = sqlx::query_as::<_, EnrichmentRunRow>(
            r"SELECT * FROM enrichment_runs
              WHERE function_id = ? AND status = 'running'
              ORDER BY created_at DESC LIMIT 1",
        )
        .bind(function_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Boot cleanup: runs still 'running' from a previous process crashed.
    pub async fn fail_stuck_enrichment_runs(&self) -> StoreResult<u64> {
        let result = sqlx::query(
            r"UPDATE enrichment_runs
              SET status = 'failed', error = 'process restarted mid-run',
                  finished_at = datetime('now')
              WHERE status = 'running'",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // =====================================================
    // Enrichment cache
    // =====================================================

    /// Bulk cache lookup, chunked to stay under SQLite's bind-parameter limit.
    pub async fn get_cached_cells(
        &self,
        function_id: &str,
        spec_hash: &str,
        input_hashes: &[String],
    ) -> StoreResult<Vec<EnrichmentCacheRow>> {
        const CHUNK: usize = 500;
        let mut out = Vec::new();
        for chunk in input_hashes.chunks(CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT * FROM enrichment_cache
                 WHERE function_id = ? AND spec_hash = ? AND input_hash IN ({placeholders})"
            );
            let mut query = sqlx::query_as::<_, EnrichmentCacheRow>(&sql)
                .bind(function_id)
                .bind(spec_hash);
            for hash in chunk {
                query = query.bind(hash);
            }
            out.extend(query.fetch_all(&self.pool).await?);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_cached_cell(
        &self,
        function_id: &str,
        spec_hash: &str,
        input_hash: &str,
        status: &str,
        value_json: Option<&str>,
        error: Option<&str>,
        prompt_tokens: Option<i64>,
        completion_tokens: Option<i64>,
        version: i64,
    ) -> StoreResult<()> {
        sqlx::query(
            r"INSERT INTO enrichment_cache
                (function_id, spec_hash, input_hash, status, value_json, error,
                 prompt_tokens, completion_tokens, version)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
              ON CONFLICT (function_id, spec_hash, input_hash) DO UPDATE SET
                status = excluded.status,
                value_json = excluded.value_json,
                error = excluded.error,
                prompt_tokens = excluded.prompt_tokens,
                completion_tokens = excluded.completion_tokens,
                version = excluded.version,
                created_at = datetime('now')",
        )
        .bind(function_id)
        .bind(spec_hash)
        .bind(input_hash)
        .bind(status)
        .bind(value_json)
        .bind(error)
        .bind(prompt_tokens)
        .bind(completion_tokens)
        .bind(version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Cached cells for one spec: (total, errors).
    pub async fn count_cached(
        &self,
        function_id: &str,
        spec_hash: &str,
    ) -> StoreResult<(i64, i64)> {
        let (total, errors): (i64, i64) = sqlx::query_as(
            r"SELECT COUNT(*), COALESCE(SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), 0)
              FROM enrichment_cache WHERE function_id = ? AND spec_hash = ?",
        )
        .bind(function_id)
        .bind(spec_hash)
        .fetch_one(&self.pool)
        .await?;
        Ok((total, errors))
    }

    /// p75 of per-cell total tokens across the function's cache history —
    /// the basis for run cost estimates. None when no token data exists.
    pub async fn cache_stats_p75_tokens(&self, function_id: &str) -> StoreResult<Option<i64>> {
        let (n,): (i64,) = sqlx::query_as(
            r"SELECT COUNT(*) FROM enrichment_cache
              WHERE function_id = ? AND prompt_tokens IS NOT NULL",
        )
        .bind(function_id)
        .fetch_one(&self.pool)
        .await?;
        if n == 0 {
            return Ok(None);
        }
        let offset = n * 3 / 4;
        let row: Option<(i64,)> = sqlx::query_as(
            r"SELECT COALESCE(prompt_tokens, 0) + COALESCE(completion_tokens, 0) AS t
              FROM enrichment_cache
              WHERE function_id = ? AND prompt_tokens IS NOT NULL
              ORDER BY t LIMIT 1 OFFSET ?",
        )
        .bind(function_id)
        .bind(offset)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(t,)| t))
    }

    /// Clear all cache rows for one spec (scope=all pre-processing).
    pub async fn delete_cache_for_spec(
        &self,
        function_id: &str,
        spec_hash: &str,
    ) -> StoreResult<u64> {
        let result =
            sqlx::query("DELETE FROM enrichment_cache WHERE function_id = ? AND spec_hash = ?")
                .bind(function_id)
                .bind(spec_hash)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    /// Clear only error rows for one spec (scope=failed pre-processing).
    pub async fn delete_error_cache_for_spec(
        &self,
        function_id: &str,
        spec_hash: &str,
    ) -> StoreResult<u64> {
        let result = sqlx::query(
            r"DELETE FROM enrichment_cache
              WHERE function_id = ? AND spec_hash = ? AND status = 'error'",
        )
        .bind(function_id)
        .bind(spec_hash)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Housekeeping on version bump: keep only rows for the given spec hashes
    /// (current + previous), drop the rest.
    pub async fn prune_cache_except(
        &self,
        function_id: &str,
        keep_spec_hashes: &[String],
    ) -> StoreResult<u64> {
        let placeholders = vec!["?"; keep_spec_hashes.len()].join(", ");
        let sql = if keep_spec_hashes.is_empty() {
            "DELETE FROM enrichment_cache WHERE function_id = ?".to_string()
        } else {
            format!(
                "DELETE FROM enrichment_cache
                 WHERE function_id = ? AND spec_hash NOT IN ({placeholders})"
            )
        };
        let mut query = sqlx::query(&sql).bind(function_id);
        for hash in keep_spec_hashes {
            query = query.bind(hash);
        }
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }
}

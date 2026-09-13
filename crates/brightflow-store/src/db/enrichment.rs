//! Enrichment persistence: functions with immutable versions, runs, and the
//! LLM output cache.

use super::StoreDb;
use crate::error::StoreResult;
use crate::models::{
    EnrichmentCacheRow, EnrichmentFunctionRow, EnrichmentFunctionVersionRow, EnrichmentRunRow,
    RecentEnrichmentRunRow,
};
use crate::row::{execute, fetch_all, fetch_one, fetch_optional};
use rusqlite::params;

impl StoreDb {
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
        let table_id = table_id.to_owned();
        let name = name.to_owned();
        let kind = kind.to_owned();
        let status = status.to_owned();
        let config_json = config_json.to_owned();
        let row = self
            .pool
            .transaction(move |tx| {
                let row: EnrichmentFunctionRow = fetch_one(
                    tx,
                    r"INSERT INTO enrichment_functions (id, table_id, name, kind, status, current_version)
              VALUES (?, ?, ?, ?, ?, 1)
              RETURNING *",
                    params![id, table_id, name, kind, status],
                )?;
                execute(
                    tx,
                    r"INSERT INTO enrichment_function_versions (function_id, version, config_json)
              VALUES (?, 1, ?)",
                    params![id, config_json],
                )?;
                Ok(row)
            })
            .await?;
        Ok(row)
    }

    pub async fn get_enrichment_function(
        &self,
        id: &str,
    ) -> StoreResult<Option<EnrichmentFunctionRow>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<EnrichmentFunctionRow, _>(
                    conn,
                    "SELECT * FROM enrichment_functions WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_enrichment_functions(
        &self,
        table_id: &str,
    ) -> StoreResult<Vec<EnrichmentFunctionRow>> {
        let table_id = table_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<EnrichmentFunctionRow, _>(
                    conn,
                    "SELECT * FROM enrichment_functions WHERE table_id = ? ORDER BY created_at",
                    params![table_id],
                )
            })
            .await?;
        Ok(rows)
    }

    /// Promoted functions of one kind for a table — the sync-time gate.
    pub async fn list_promoted_functions(
        &self,
        table_id: &str,
        kind: &str,
    ) -> StoreResult<Vec<EnrichmentFunctionRow>> {
        let table_id = table_id.to_owned();
        let kind = kind.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<EnrichmentFunctionRow, _>(
                    conn,
                    r"SELECT * FROM enrichment_functions
              WHERE table_id = ? AND kind = ? AND status = 'promoted'
              ORDER BY created_at",
                    params![table_id, kind],
                )
            })
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
        let table_id = table_id.to_owned();
        let kind = kind.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<(String,), _>(
                    conn,
                    r"SELECT v.config_json
              FROM enrichment_functions f
              JOIN enrichment_function_versions v
                ON v.function_id = f.id AND v.version = f.current_version
              WHERE f.table_id = ? AND f.kind = ? AND f.status = 'promoted'
              ORDER BY f.created_at
              LIMIT 1",
                    params![table_id, kind],
                )
            })
            .await?;
        Ok(row.map(|(json,)| json))
    }

    pub async fn update_enrichment_function_config(
        &self,
        id: &str,
        config_json: &str,
    ) -> StoreResult<i64> {
        let id = id.to_owned();
        let config_json = config_json.to_owned();
        let next = self
            .pool
            .transaction(move |tx| {
                // The version read must stay inside the transaction: two
                // concurrent updates would otherwise compute the same `next`
                // and collide on the versions primary key. `SqlitePool::
                // transaction` opens BEGIN IMMEDIATE, so the second caller
                // waits on the write lock and reads a version this one already
                // committed, rather than racing it.
                let (next,): (i64,) = fetch_one(
                    tx,
                    "SELECT current_version + 1 FROM enrichment_functions WHERE id = ?",
                    params![id],
                )?;
                execute(
                    tx,
                    r"INSERT INTO enrichment_function_versions (function_id, version, config_json)
              VALUES (?, ?, ?)",
                    params![id, next, config_json],
                )?;
                execute(
                    tx,
                    r"UPDATE enrichment_functions
              SET current_version = ?, updated_at = datetime('now') WHERE id = ?",
                    params![next, id],
                )?;
                Ok(next)
            })
            .await?;
        Ok(next)
    }

    pub async fn set_enrichment_function_status(
        &self,
        id: &str,
        status: &str,
    ) -> StoreResult<bool> {
        let id = id.to_owned();
        let status = status.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE enrichment_functions
              SET status = ?, updated_at = datetime('now') WHERE id = ?",
                    params![status, id],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    /// Delete a function; versions, runs and cache cascade away.
    pub async fn delete_enrichment_function(&self, id: &str) -> StoreResult<bool> {
        let id = id.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM enrichment_functions WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(affected > 0)
    }

    pub async fn list_enrichment_function_versions(
        &self,
        function_id: &str,
    ) -> StoreResult<Vec<EnrichmentFunctionVersionRow>> {
        let function_id = function_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<EnrichmentFunctionVersionRow, _>(
                    conn,
                    r"SELECT * FROM enrichment_function_versions
              WHERE function_id = ? ORDER BY version DESC",
                    params![function_id],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn get_enrichment_function_version(
        &self,
        function_id: &str,
        version: i64,
    ) -> StoreResult<Option<EnrichmentFunctionVersionRow>> {
        let function_id = function_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<EnrichmentFunctionVersionRow, _>(
                    conn,
                    "SELECT * FROM enrichment_function_versions WHERE function_id = ? AND version = ?",
                    params![function_id, version],
                )
            })
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
        let function_id = function_id.to_owned();
        let mode = mode.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<EnrichmentRunRow, _>(
                    conn,
                    r"INSERT INTO enrichment_runs (id, function_id, version, mode, rows_total)
              VALUES (?, ?, ?, ?, ?)
              RETURNING *",
                    params![id, function_id, version, mode, rows_total],
                )
            })
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
        cached_tokens: i64,
    ) -> StoreResult<()> {
        let id = id.to_owned();
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE enrichment_runs
              SET rows_done = ?, rows_failed = ?, rows_cached = ?,
                  prompt_tokens = ?, completion_tokens = ?, cached_tokens = ?,
                  total_tokens = ? + ?
              WHERE id = ?",
                    params![
                        rows_done,
                        rows_failed,
                        rows_cached,
                        prompt_tokens,
                        completion_tokens,
                        cached_tokens,
                        prompt_tokens,
                        completion_tokens,
                        id
                    ],
                )
                .map(|_| ())
            })
            .await?;
        Ok(())
    }

    pub async fn finish_enrichment_run(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> StoreResult<()> {
        let id = id.to_owned();
        let status = status.to_owned();
        let error = error.map(str::to_owned);
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE enrichment_runs
              SET status = ?, error = ?, finished_at = datetime('now') WHERE id = ?",
                    params![status, error, id],
                )
                .map(|_| ())
            })
            .await?;
        Ok(())
    }

    /// The most recent runs across every function, newest first, each
    /// with the function's name and the table it belongs to.
    pub async fn list_recent_enrichment_runs(
        &self,
        limit: i64,
    ) -> StoreResult<Vec<RecentEnrichmentRunRow>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<RecentEnrichmentRunRow, _>(
                    conn,
                    r"SELECT r.id, r.function_id, f.name AS function_name,
                             t.source_id, t.name AS table_name,
                             r.mode, r.status, r.rows_total, r.rows_done, r.rows_failed,
                             r.error, r.created_at, r.finished_at
                      FROM enrichment_runs r
                      JOIN enrichment_functions f ON f.id = r.function_id
                      JOIN tables t ON t.id = f.table_id
                      ORDER BY r.created_at DESC, r.id DESC LIMIT ?",
                    params![limit],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn get_enrichment_run(&self, id: &str) -> StoreResult<Option<EnrichmentRunRow>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<EnrichmentRunRow, _>(
                    conn,
                    "SELECT * FROM enrichment_runs WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn active_enrichment_run(
        &self,
        function_id: &str,
    ) -> StoreResult<Option<EnrichmentRunRow>> {
        let function_id = function_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<EnrichmentRunRow, _>(
                    conn,
                    r"SELECT * FROM enrichment_runs
              WHERE function_id = ? AND status = 'running'
              ORDER BY created_at DESC LIMIT 1",
                    params![function_id],
                )
            })
            .await?;
        Ok(row)
    }

    /// Boot cleanup: runs still 'running' from a previous process crashed.
    pub async fn fail_stuck_enrichment_runs(&self) -> StoreResult<u64> {
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    r"UPDATE enrichment_runs
              SET status = 'failed', error = 'process restarted mid-run',
                  finished_at = datetime('now')
              WHERE status = 'running'",
                    params![],
                )
            })
            .await?;
        Ok(affected)
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
        let function_id = function_id.to_owned();
        let spec_hash = spec_hash.to_owned();
        let input_hashes = input_hashes.to_vec();
        // All chunks run on one pooled connection: same behavior per chunk,
        // fewer pool round-trips than checking out a connection per chunk.
        let out = self
            .pool
            .call(move |conn| {
                let mut out = Vec::new();
                for chunk in input_hashes.chunks(CHUNK) {
                    let placeholders = vec!["?"; chunk.len()].join(", ");
                    let sql = format!(
                        "SELECT * FROM enrichment_cache
                 WHERE function_id = ? AND spec_hash = ? AND input_hash IN ({placeholders})"
                    );
                    let binds = rusqlite::params_from_iter(
                        std::iter::once(function_id.clone())
                            .chain(std::iter::once(spec_hash.clone()))
                            .chain(chunk.iter().cloned()),
                    );
                    out.extend(fetch_all::<EnrichmentCacheRow, _>(conn, &sql, binds)?);
                }
                Ok(out)
            })
            .await?;
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
        cached_tokens: Option<i64>,
        version: i64,
    ) -> StoreResult<()> {
        let function_id = function_id.to_owned();
        let spec_hash = spec_hash.to_owned();
        let input_hash = input_hash.to_owned();
        let status = status.to_owned();
        let value_json = value_json.map(str::to_owned);
        let error = error.map(str::to_owned);
        self.pool
            .call(move |conn| {
                execute(
                    conn,
                    r"INSERT INTO enrichment_cache
                (function_id, spec_hash, input_hash, status, value_json, error,
                 prompt_tokens, completion_tokens, cached_tokens, version)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
              ON CONFLICT (function_id, spec_hash, input_hash) DO UPDATE SET
                status = excluded.status,
                value_json = excluded.value_json,
                error = excluded.error,
                prompt_tokens = excluded.prompt_tokens,
                completion_tokens = excluded.completion_tokens,
                cached_tokens = excluded.cached_tokens,
                version = excluded.version,
                created_at = datetime('now')",
                    params![
                        function_id,
                        spec_hash,
                        input_hash,
                        status,
                        value_json,
                        error,
                        prompt_tokens,
                        completion_tokens,
                        cached_tokens,
                        version
                    ],
                )
                .map(|_| ())
            })
            .await?;
        Ok(())
    }

    /// Cached cells for one spec: (total, errors).
    pub async fn count_cached(
        &self,
        function_id: &str,
        spec_hash: &str,
    ) -> StoreResult<(i64, i64)> {
        let function_id = function_id.to_owned();
        let spec_hash = spec_hash.to_owned();
        let (total, errors) = self
            .pool
            .call(move |conn| {
                fetch_one::<(i64, i64), _>(
                    conn,
                    r"SELECT COUNT(*), COALESCE(SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), 0)
              FROM enrichment_cache WHERE function_id = ? AND spec_hash = ?",
                    params![function_id, spec_hash],
                )
            })
            .await?;
        Ok((total, errors))
    }

    /// p75 of per-cell total tokens across the function's cache history —
    /// the basis for run cost estimates. None when no token data exists.
    pub async fn cache_stats_p75_tokens(&self, function_id: &str) -> StoreResult<Option<i64>> {
        let function_id = function_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                let (n,): (i64,) = fetch_one(
                    conn,
                    r"SELECT COUNT(*) FROM enrichment_cache
              WHERE function_id = ? AND prompt_tokens IS NOT NULL",
                    params![function_id],
                )?;
                if n == 0 {
                    return Ok(None);
                }
                let offset = n * 3 / 4;
                fetch_optional::<(i64,), _>(
                    conn,
                    r"SELECT COALESCE(prompt_tokens, 0) + COALESCE(completion_tokens, 0) AS t
              FROM enrichment_cache
              WHERE function_id = ? AND prompt_tokens IS NOT NULL
              ORDER BY t LIMIT 1 OFFSET ?",
                    params![function_id, offset],
                )
            })
            .await?;
        Ok(row.map(|(t,)| t))
    }

    /// Clear all cache rows for one spec (scope=all pre-processing).
    pub async fn delete_cache_for_spec(
        &self,
        function_id: &str,
        spec_hash: &str,
    ) -> StoreResult<u64> {
        let function_id = function_id.to_owned();
        let spec_hash = spec_hash.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM enrichment_cache WHERE function_id = ? AND spec_hash = ?",
                    params![function_id, spec_hash],
                )
            })
            .await?;
        Ok(affected)
    }

    /// Clear only error rows for one spec (scope=failed pre-processing).
    pub async fn delete_error_cache_for_spec(
        &self,
        function_id: &str,
        spec_hash: &str,
    ) -> StoreResult<u64> {
        let function_id = function_id.to_owned();
        let spec_hash = spec_hash.to_owned();
        let affected = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    r"DELETE FROM enrichment_cache
              WHERE function_id = ? AND spec_hash = ? AND status = 'error'",
                    params![function_id, spec_hash],
                )
            })
            .await?;
        Ok(affected)
    }

    /// Housekeeping on version bump: keep only rows for the given spec hashes
    /// (current + previous), drop the rest.
    pub async fn prune_cache_except(
        &self,
        function_id: &str,
        keep_spec_hashes: &[String],
    ) -> StoreResult<u64> {
        let function_id = function_id.to_owned();
        let keep_spec_hashes = keep_spec_hashes.to_vec();
        let affected = self
            .pool
            .call(move |conn| {
                let placeholders = vec!["?"; keep_spec_hashes.len()].join(", ");
                let sql = if keep_spec_hashes.is_empty() {
                    "DELETE FROM enrichment_cache WHERE function_id = ?".to_string()
                } else {
                    format!(
                        "DELETE FROM enrichment_cache
                 WHERE function_id = ? AND spec_hash NOT IN ({placeholders})"
                    )
                };
                let binds = rusqlite::params_from_iter(
                    std::iter::once(function_id).chain(keep_spec_hashes),
                );
                execute(conn, &sql, binds)
            })
            .await?;
        Ok(affected)
    }
}

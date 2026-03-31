use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::SchedulerResult;
use crate::models::{ConnectorConfig, SchedulerJob, SyncRun, SyncState};

#[derive(Clone)]
pub struct SchedulerDb {
    pool: SqlitePool,
}

impl SchedulerDb {
    pub async fn new(database_url: &str) -> SchedulerResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .pragma("synchronous", "NORMAL")
            .pragma("cache_size", "-64000")
            .pragma("mmap_size", "268435456")
            .pragma("temp_store", "MEMORY")
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    // =====================================================
    // Connector Config CRUD
    // =====================================================

    pub async fn create_connector_config(
        &self,
        name: &str,
        connector_path: &str,
        config_json: &str,
    ) -> SchedulerResult<ConnectorConfig> {
        let id = uuid::Uuid::new_v4().to_string();
        let row = sqlx::query_as::<_, ConnectorConfig>(
            r"INSERT INTO connector_configs (id, name, connector_path, config_json)
              VALUES (?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(name)
        .bind(connector_path)
        .bind(config_json)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_connector_config(&self, id: &str) -> SchedulerResult<Option<ConnectorConfig>> {
        let row =
            sqlx::query_as::<_, ConnectorConfig>("SELECT * FROM connector_configs WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    pub async fn get_connector_config_by_name(
        &self,
        name: &str,
    ) -> SchedulerResult<Option<ConnectorConfig>> {
        let row =
            sqlx::query_as::<_, ConnectorConfig>("SELECT * FROM connector_configs WHERE name = ?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    pub async fn list_connector_configs(&self) -> SchedulerResult<Vec<ConnectorConfig>> {
        let rows =
            sqlx::query_as::<_, ConnectorConfig>("SELECT * FROM connector_configs ORDER BY name")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    pub async fn update_connector_config(
        &self,
        id: &str,
        name: &str,
        connector_path: &str,
        config_json: &str,
    ) -> SchedulerResult<Option<ConnectorConfig>> {
        let row = sqlx::query_as::<_, ConnectorConfig>(
            r"UPDATE connector_configs
              SET name = ?, connector_path = ?, config_json = ?, updated_at = datetime('now')
              WHERE id = ?
              RETURNING *",
        )
        .bind(name)
        .bind(connector_path)
        .bind(config_json)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_connector_config(&self, id: &str) -> SchedulerResult<bool> {
        let result = sqlx::query("DELETE FROM connector_configs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
    // Scheduler Job CRUD
    // =====================================================

    pub async fn create_scheduler_job(
        &self,
        name: &str,
        connector_id: &str,
        interval_secs: i64,
    ) -> SchedulerResult<SchedulerJob> {
        let id = uuid::Uuid::new_v4().to_string();
        let row = sqlx::query_as::<_, SchedulerJob>(
            r"INSERT INTO scheduler_jobs (id, name, connector_id, interval_secs)
              VALUES (?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(name)
        .bind(connector_id)
        .bind(interval_secs)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_scheduler_job(&self, id: &str) -> SchedulerResult<Option<SchedulerJob>> {
        let row = sqlx::query_as::<_, SchedulerJob>("SELECT * FROM scheduler_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_scheduler_jobs(&self) -> SchedulerResult<Vec<SchedulerJob>> {
        let rows =
            sqlx::query_as::<_, SchedulerJob>("SELECT * FROM scheduler_jobs ORDER BY created_at")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    pub async fn list_enabled_scheduler_jobs(&self) -> SchedulerResult<Vec<SchedulerJob>> {
        let rows = sqlx::query_as::<_, SchedulerJob>(
            "SELECT * FROM scheduler_jobs WHERE enabled = 1 ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn update_scheduler_job(
        &self,
        id: &str,
        interval_secs: Option<i64>,
        enabled: Option<bool>,
    ) -> SchedulerResult<Option<SchedulerJob>> {
        let row = sqlx::query_as::<_, SchedulerJob>(
            r"UPDATE scheduler_jobs
              SET interval_secs = COALESCE(?, interval_secs),
                  enabled = COALESCE(?, enabled),
                  updated_at = datetime('now')
              WHERE id = ?
              RETURNING *",
        )
        .bind(interval_secs)
        .bind(enabled)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_scheduler_job(&self, id: &str) -> SchedulerResult<bool> {
        let result = sqlx::query("DELETE FROM scheduler_jobs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // =====================================================
    // Sync State
    // =====================================================

    pub async fn get_sync_state(
        &self,
        connector_id: &str,
        endpoint: &str,
    ) -> SchedulerResult<Option<SyncState>> {
        let row = sqlx::query_as::<_, SyncState>(
            "SELECT * FROM sync_state WHERE connector_id = ? AND endpoint = ?",
        )
        .bind(connector_id)
        .bind(endpoint)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_sync_states(&self, connector_id: &str) -> SchedulerResult<Vec<SyncState>> {
        let rows = sqlx::query_as::<_, SyncState>(
            "SELECT * FROM sync_state WHERE connector_id = ? ORDER BY endpoint",
        )
        .bind(connector_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn upsert_sync_state(
        &self,
        connector_id: &str,
        endpoint: &str,
        cursor_field: Option<&str>,
        cursor_value: Option<&str>,
        status: &str,
        rows_synced: i64,
    ) -> SchedulerResult<SyncState> {
        let row = sqlx::query_as::<_, SyncState>(
            r"INSERT INTO sync_state (connector_id, endpoint, cursor_field, cursor_value, last_sync_at, last_sync_status, rows_synced)
              VALUES (?, ?, ?, ?, datetime('now'), ?, ?)
              ON CONFLICT(connector_id, endpoint) DO UPDATE SET
                cursor_field = COALESCE(excluded.cursor_field, sync_state.cursor_field),
                cursor_value = COALESCE(excluded.cursor_value, sync_state.cursor_value),
                last_sync_at = excluded.last_sync_at,
                last_sync_status = excluded.last_sync_status,
                rows_synced = excluded.rows_synced
              RETURNING *",
        )
        .bind(connector_id)
        .bind(endpoint)
        .bind(cursor_field)
        .bind(cursor_value)
        .bind(status)
        .bind(rows_synced)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    // =====================================================
    // Sync Runs
    // =====================================================

    pub async fn create_sync_run(
        &self,
        job_id: Option<&str>,
        connector_id: &str,
        status: &str,
    ) -> SchedulerResult<SyncRun> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, SyncRun>(
            r"INSERT INTO sync_runs (id, job_id, connector_id, started_at, status)
              VALUES (?, ?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(job_id)
        .bind(connector_id)
        .bind(&now)
        .bind(status)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn update_sync_run(
        &self,
        id: &str,
        status: &str,
        endpoints_synced: Option<&str>,
        rows_synced: i64,
        error: Option<&str>,
    ) -> SchedulerResult<Option<SyncRun>> {
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, SyncRun>(
            r"UPDATE sync_runs
              SET status = ?, finished_at = ?,
                  endpoints_synced = ?, rows_synced = ?, error = ?
              WHERE id = ?
              RETURNING *",
        )
        .bind(status)
        .bind(&now)
        .bind(endpoints_synced)
        .bind(rows_synced)
        .bind(error)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn delete_stale_sync_runs(&self) -> SchedulerResult<u64> {
        let result = sqlx::query(
            "DELETE FROM sync_runs WHERE status IN ('running', 'pending') OR error = 'Interrupted by server restart'",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn get_sync_run(&self, id: &str) -> SchedulerResult<Option<SyncRun>> {
        let row = sqlx::query_as::<_, SyncRun>("SELECT * FROM sync_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_sync_runs(&self, limit: i64) -> SchedulerResult<Vec<SyncRun>> {
        let rows = sqlx::query_as::<_, SyncRun>(
            "SELECT * FROM sync_runs ORDER BY started_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_latest_sync_run_for_job(
        &self,
        job_id: &str,
    ) -> SchedulerResult<Option<SyncRun>> {
        let row = sqlx::query_as::<_, SyncRun>(
            "SELECT * FROM sync_runs WHERE job_id = ? ORDER BY started_at DESC LIMIT 1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_latest_sync_run_for_connector(
        &self,
        connector_id: &str,
    ) -> SchedulerResult<Option<SyncRun>> {
        let row = sqlx::query_as::<_, SyncRun>(
            "SELECT * FROM sync_runs WHERE connector_id = ? ORDER BY started_at DESC LIMIT 1",
        )
        .bind(connector_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_sync_runs_for_connector(
        &self,
        connector_id: &str,
        limit: i64,
    ) -> SchedulerResult<Vec<SyncRun>> {
        let rows = sqlx::query_as::<_, SyncRun>(
            "SELECT * FROM sync_runs WHERE connector_id = ? ORDER BY started_at DESC LIMIT ?",
        )
        .bind(connector_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

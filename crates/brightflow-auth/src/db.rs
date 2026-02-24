use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::AuthResult;
use crate::models::{ConnectorConfig, SchedulerJob, SyncRun, SyncState, User, UserSettings};

#[derive(Clone)]
pub struct AuthDb {
    pool: SqlitePool,
}

impl AuthDb {
    pub async fn new(database_url: &str) -> AuthResult<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn create_user(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
        is_admin: bool,
    ) -> AuthResult<User> {
        let id = uuid::Uuid::new_v4().to_string();
        let user = sqlx::query_as::<_, User>(
            r"INSERT INTO users (id, email, display_name, password_hash, is_admin)
              VALUES (?, ?, ?, ?, ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .bind(is_admin)
        .fetch_one(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn get_user_by_id(&self, id: &str) -> AuthResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    pub async fn get_user_by_email(&self, email: &str) -> AuthResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    pub async fn user_count(&self) -> AuthResult<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    pub async fn get_data_mode(&self, user_id: &str) -> AuthResult<String> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT data_mode FROM user_settings WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map_or_else(|| "memory".to_string(), |r| r.0))
    }

    pub async fn upsert_user_settings(
        &self,
        user_id: &str,
        data_mode: &str,
    ) -> AuthResult<UserSettings> {
        let settings = sqlx::query_as::<_, UserSettings>(
            r"INSERT INTO user_settings (user_id, data_mode, updated_at)
              VALUES (?, ?, datetime('now'))
              ON CONFLICT(user_id) DO UPDATE SET
                data_mode = excluded.data_mode,
                updated_at = excluded.updated_at
              RETURNING *",
        )
        .bind(user_id)
        .bind(data_mode)
        .fetch_one(&self.pool)
        .await?;

        Ok(settings)
    }

    // =====================================================
    // Connector Config CRUD
    // =====================================================

    pub async fn create_connector_config(
        &self,
        name: &str,
        connector_path: &str,
        config_json: &str,
    ) -> AuthResult<ConnectorConfig> {
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

    pub async fn get_connector_config(&self, id: &str) -> AuthResult<Option<ConnectorConfig>> {
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
    ) -> AuthResult<Option<ConnectorConfig>> {
        let row =
            sqlx::query_as::<_, ConnectorConfig>("SELECT * FROM connector_configs WHERE name = ?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    pub async fn list_connector_configs(&self) -> AuthResult<Vec<ConnectorConfig>> {
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
    ) -> AuthResult<Option<ConnectorConfig>> {
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

    pub async fn delete_connector_config(&self, id: &str) -> AuthResult<bool> {
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
    ) -> AuthResult<SchedulerJob> {
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

    pub async fn get_scheduler_job(&self, id: &str) -> AuthResult<Option<SchedulerJob>> {
        let row = sqlx::query_as::<_, SchedulerJob>("SELECT * FROM scheduler_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_scheduler_jobs(&self) -> AuthResult<Vec<SchedulerJob>> {
        let rows =
            sqlx::query_as::<_, SchedulerJob>("SELECT * FROM scheduler_jobs ORDER BY created_at")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    pub async fn list_enabled_scheduler_jobs(&self) -> AuthResult<Vec<SchedulerJob>> {
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
    ) -> AuthResult<Option<SchedulerJob>> {
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

    pub async fn delete_scheduler_job(&self, id: &str) -> AuthResult<bool> {
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
    ) -> AuthResult<Option<SyncState>> {
        let row = sqlx::query_as::<_, SyncState>(
            "SELECT * FROM sync_state WHERE connector_id = ? AND endpoint = ?",
        )
        .bind(connector_id)
        .bind(endpoint)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_sync_states(&self, connector_id: &str) -> AuthResult<Vec<SyncState>> {
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
    ) -> AuthResult<SyncState> {
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
    ) -> AuthResult<SyncRun> {
        let id = uuid::Uuid::new_v4().to_string();
        let row = sqlx::query_as::<_, SyncRun>(
            r"INSERT INTO sync_runs (id, job_id, connector_id, started_at, status)
              VALUES (?, ?, ?, datetime('now'), ?)
              RETURNING *",
        )
        .bind(&id)
        .bind(job_id)
        .bind(connector_id)
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
    ) -> AuthResult<Option<SyncRun>> {
        let row = sqlx::query_as::<_, SyncRun>(
            r"UPDATE sync_runs
              SET status = ?, finished_at = datetime('now'),
                  endpoints_synced = ?, rows_synced = ?, error = ?
              WHERE id = ?
              RETURNING *",
        )
        .bind(status)
        .bind(endpoints_synced)
        .bind(rows_synced)
        .bind(error)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_sync_run(&self, id: &str) -> AuthResult<Option<SyncRun>> {
        let row = sqlx::query_as::<_, SyncRun>("SELECT * FROM sync_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_sync_runs(&self, limit: i64) -> AuthResult<Vec<SyncRun>> {
        let rows = sqlx::query_as::<_, SyncRun>(
            "SELECT * FROM sync_runs ORDER BY started_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_latest_sync_run_for_job(&self, job_id: &str) -> AuthResult<Option<SyncRun>> {
        let row = sqlx::query_as::<_, SyncRun>(
            "SELECT * FROM sync_runs WHERE job_id = ? ORDER BY started_at DESC LIMIT 1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}

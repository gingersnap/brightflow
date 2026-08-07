//! SQLite persistence for connector configs, jobs, sync cursors, and run history.
//!
//! Owns its own pool and migrations so the scheduler can be constructed from a
//! URL alone. All four tables are small and read far more often than written, so
//! the pragmas favour read latency (WAL, generous cache, mmap) over write
//! durability — `synchronous = NORMAL` can lose the last commits on power loss.
//!
//! That trade is acceptable because nothing here is a source of truth. Lost
//! sync-run history is only reporting. A lost cursor is not repaired but is
//! self-correcting: with no cursor for an endpoint the next sync re-fetches it
//! from the beginning — slower, not wrong.

use brightflow_store::rusqlite::params;
use brightflow_store::{
    execute, fetch_all, fetch_one, fetch_optional, migration, Migration, SqlitePool,
};

use crate::error::SchedulerResult;
use crate::models::{ConnectorConfig, SchedulerJob, SyncRun, SyncState};

/// The scheduler's migration list, in apply order. Append-only: add a new
/// file and a new entry, never edit or reorder existing ones.
static MIGRATIONS: &[Migration] = &[
    migration!(1, "001_create_connector_configs"),
    migration!(2, "002_create_scheduler_jobs"),
    migration!(3, "003_create_sync_state"),
    migration!(4, "004_create_sync_runs"),
    migration!(5, "005_add_token_column"),
    migration!(6, "006_sync_runs_cascade"),
];

#[derive(Clone, Debug)]
pub struct SchedulerDb {
    pool: SqlitePool,
}

impl SchedulerDb {
    pub async fn new(database_url: &str) -> SchedulerResult<Self> {
        let pool = brightflow_store::open_pool(
            database_url,
            brightflow_store::SqlitePoolProfile::METADATA,
        )
        .await?;

        brightflow_store::migrate(&pool, MIGRATIONS).await?;

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
        token: Option<&str>,
    ) -> SchedulerResult<ConnectorConfig> {
        let id = uuid::Uuid::new_v4().to_string();
        let name = name.to_owned();
        let connector_path = connector_path.to_owned();
        let config_json = config_json.to_owned();
        let token = token.map(str::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<ConnectorConfig, _>(
                    conn,
                    r"INSERT INTO connector_configs (id, name, connector_path, config_json, token)
                      VALUES (?, ?, ?, ?, ?)
                      RETURNING *",
                    params![id, name, connector_path, config_json, token],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_connector_config(&self, id: &str) -> SchedulerResult<Option<ConnectorConfig>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ConnectorConfig, _>(
                    conn,
                    "SELECT * FROM connector_configs WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_connector_config_by_name(
        &self,
        name: &str,
    ) -> SchedulerResult<Option<ConnectorConfig>> {
        let name = name.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ConnectorConfig, _>(
                    conn,
                    "SELECT * FROM connector_configs WHERE name = ?",
                    params![name],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_connector_configs(&self) -> SchedulerResult<Vec<ConnectorConfig>> {
        let rows = self
            .pool
            .call(|conn| {
                fetch_all::<ConnectorConfig, _>(
                    conn,
                    "SELECT * FROM connector_configs ORDER BY name",
                    [],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn update_connector_config(
        &self,
        id: &str,
        name: &str,
        connector_path: &str,
        config_json: &str,
        token: Option<&str>,
    ) -> SchedulerResult<Option<ConnectorConfig>> {
        let id = id.to_owned();
        let name = name.to_owned();
        let connector_path = connector_path.to_owned();
        let config_json = config_json.to_owned();
        let token = token.map(str::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ConnectorConfig, _>(
                    conn,
                    r"UPDATE connector_configs
                      SET name = ?, connector_path = ?, config_json = ?, token = COALESCE(?, token),
                          updated_at = datetime('now')
                      WHERE id = ?
                      RETURNING *",
                    params![name, connector_path, config_json, token, id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn update_connector_token(
        &self,
        id: &str,
        token: &str,
    ) -> SchedulerResult<Option<ConnectorConfig>> {
        let id = id.to_owned();
        let token = token.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<ConnectorConfig, _>(
                    conn,
                    r"UPDATE connector_configs
                      SET token = ?, updated_at = datetime('now')
                      WHERE id = ?
                      RETURNING *",
                    params![token, id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn delete_connector_config(&self, id: &str) -> SchedulerResult<bool> {
        let id = id.to_owned();
        let n = self
            .pool
            .call(move |conn| {
                execute(
                    conn,
                    "DELETE FROM connector_configs WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(n > 0)
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
        let name = name.to_owned();
        let connector_id = connector_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<SchedulerJob, _>(
                    conn,
                    r"INSERT INTO scheduler_jobs (id, name, connector_id, interval_secs)
                      VALUES (?, ?, ?, ?)
                      RETURNING *",
                    params![id, name, connector_id, interval_secs],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_scheduler_job(&self, id: &str) -> SchedulerResult<Option<SchedulerJob>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SchedulerJob, _>(
                    conn,
                    "SELECT * FROM scheduler_jobs WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_scheduler_jobs(&self) -> SchedulerResult<Vec<SchedulerJob>> {
        let rows = self
            .pool
            .call(|conn| {
                fetch_all::<SchedulerJob, _>(
                    conn,
                    "SELECT * FROM scheduler_jobs ORDER BY created_at",
                    [],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn list_enabled_scheduler_jobs(&self) -> SchedulerResult<Vec<SchedulerJob>> {
        let rows = self
            .pool
            .call(|conn| {
                fetch_all::<SchedulerJob, _>(
                    conn,
                    "SELECT * FROM scheduler_jobs WHERE enabled = 1 ORDER BY created_at",
                    [],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn update_scheduler_job(
        &self,
        id: &str,
        interval_secs: Option<i64>,
        enabled: Option<bool>,
    ) -> SchedulerResult<Option<SchedulerJob>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SchedulerJob, _>(
                    conn,
                    r"UPDATE scheduler_jobs
                      SET interval_secs = COALESCE(?, interval_secs),
                          enabled = COALESCE(?, enabled),
                          updated_at = datetime('now')
                      WHERE id = ?
                      RETURNING *",
                    params![interval_secs, enabled, id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn delete_scheduler_job(&self, id: &str) -> SchedulerResult<bool> {
        let id = id.to_owned();
        let n = self
            .pool
            .call(move |conn| execute(conn, "DELETE FROM scheduler_jobs WHERE id = ?", params![id]))
            .await?;
        Ok(n > 0)
    }

    // =====================================================
    // Sync State
    // =====================================================

    pub async fn list_sync_states(&self, connector_id: &str) -> SchedulerResult<Vec<SyncState>> {
        let connector_id = connector_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<SyncState, _>(
                    conn,
                    "SELECT * FROM sync_state WHERE connector_id = ? ORDER BY endpoint",
                    params![connector_id],
                )
            })
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
        let connector_id = connector_id.to_owned();
        let endpoint = endpoint.to_owned();
        let cursor_field = cursor_field.map(str::to_owned);
        let cursor_value = cursor_value.map(str::to_owned);
        let status = status.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<SyncState, _>(
                    conn,
                    r"INSERT INTO sync_state (connector_id, endpoint, cursor_field, cursor_value, last_sync_at, last_sync_status, rows_synced)
                      VALUES (?, ?, ?, ?, datetime('now'), ?, ?)
                      ON CONFLICT(connector_id, endpoint) DO UPDATE SET
                        cursor_field = COALESCE(excluded.cursor_field, sync_state.cursor_field),
                        cursor_value = COALESCE(excluded.cursor_value, sync_state.cursor_value),
                        last_sync_at = excluded.last_sync_at,
                        last_sync_status = excluded.last_sync_status,
                        rows_synced = excluded.rows_synced
                      RETURNING *",
                    params![connector_id, endpoint, cursor_field, cursor_value, status, rows_synced],
                )
            })
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
        let job_id = job_id.map(str::to_owned);
        let connector_id = connector_id.to_owned();
        let status = status.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_one::<SyncRun, _>(
                    conn,
                    r"INSERT INTO sync_runs (id, job_id, connector_id, started_at, status)
                      VALUES (?, ?, ?, ?, ?)
                      RETURNING *",
                    params![id, job_id, connector_id, now, status],
                )
            })
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
        let id = id.to_owned();
        let status = status.to_owned();
        let endpoints_synced = endpoints_synced.map(str::to_owned);
        let error = error.map(str::to_owned);
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SyncRun, _>(
                    conn,
                    r"UPDATE sync_runs
                      SET status = ?, finished_at = ?,
                          endpoints_synced = ?, rows_synced = ?, error = ?
                      WHERE id = ?
                      RETURNING *",
                    params![status, now, endpoints_synced, rows_synced, error, id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn delete_stale_sync_runs(&self) -> SchedulerResult<u64> {
        let n = self
            .pool
            .call(|conn| {
                execute(
                    conn,
                    "DELETE FROM sync_runs WHERE status IN ('running', 'pending') OR error = 'Interrupted by server restart'",
                    [],
                )
            })
            .await?;
        Ok(n)
    }

    pub async fn get_sync_run(&self, id: &str) -> SchedulerResult<Option<SyncRun>> {
        let id = id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SyncRun, _>(
                    conn,
                    "SELECT * FROM sync_runs WHERE id = ?",
                    params![id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_sync_runs(&self, limit: i64) -> SchedulerResult<Vec<SyncRun>> {
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<SyncRun, _>(
                    conn,
                    "SELECT * FROM sync_runs ORDER BY started_at DESC LIMIT ?",
                    params![limit],
                )
            })
            .await?;
        Ok(rows)
    }

    pub async fn get_latest_sync_run_for_job(
        &self,
        job_id: &str,
    ) -> SchedulerResult<Option<SyncRun>> {
        let job_id = job_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SyncRun, _>(
                    conn,
                    "SELECT * FROM sync_runs WHERE job_id = ? ORDER BY started_at DESC LIMIT 1",
                    params![job_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn get_latest_sync_run_for_connector(
        &self,
        connector_id: &str,
    ) -> SchedulerResult<Option<SyncRun>> {
        let connector_id = connector_id.to_owned();
        let row = self
            .pool
            .call(move |conn| {
                fetch_optional::<SyncRun, _>(
                    conn,
                    "SELECT * FROM sync_runs WHERE connector_id = ? ORDER BY started_at DESC LIMIT 1",
                    params![connector_id],
                )
            })
            .await?;
        Ok(row)
    }

    pub async fn list_sync_runs_for_connector(
        &self,
        connector_id: &str,
        limit: i64,
    ) -> SchedulerResult<Vec<SyncRun>> {
        let connector_id = connector_id.to_owned();
        let rows = self
            .pool
            .call(move |conn| {
                fetch_all::<SyncRun, _>(
                    conn,
                    "SELECT * FROM sync_runs WHERE connector_id = ? ORDER BY started_at DESC LIMIT ?",
                    params![connector_id, limit],
                )
            })
            .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard against adding a migration file and forgetting the list entry
    /// (or vice versa): the embedded list must equal the sorted directory.
    #[test]
    fn migrations_list_matches_directory() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("migrations dir")
            .map(|e| {
                e.expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter_map(|n| n.strip_suffix(".sql").map(ToOwned::to_owned))
            .collect();
        on_disk.sort();
        let embedded: Vec<String> = MIGRATIONS.iter().map(|m| m.name.to_owned()).collect();
        assert_eq!(embedded, on_disk);
    }

    /// The RETURNING-based CRUD round-trips through a real file-backed pool —
    /// the scheduler had no db coverage at all before the rusqlite port.
    #[tokio::test]
    async fn connector_config_and_job_crud_round_trip() {
        let tmp = tempfile::tempdir().expect("tmp");
        let url = format!("sqlite:{}?mode=rwc", tmp.path().join("sched.db").display());
        let db = SchedulerDb::new(&url).await.expect("db");

        let cfg = db
            .create_connector_config("gh", "/bin/gh", "{}", Some("tok"))
            .await
            .expect("create config");
        assert_eq!(cfg.name, "gh");
        assert_eq!(cfg.token.as_deref(), Some("tok"));

        // COALESCE keeps the old token when None is passed.
        let updated = db
            .update_connector_config(&cfg.id, "gh2", "/bin/gh", "{}", None)
            .await
            .expect("update")
            .expect("exists");
        assert_eq!(updated.name, "gh2");
        assert_eq!(updated.token.as_deref(), Some("tok"));

        let job = db
            .create_scheduler_job("nightly", &cfg.id, 3600)
            .await
            .expect("create job");
        assert!(job.enabled, "jobs default to enabled");

        let toggled = db
            .update_scheduler_job(&job.id, None, Some(false))
            .await
            .expect("toggle")
            .expect("exists");
        assert!(!toggled.enabled);
        assert_eq!(toggled.interval_secs, 3600, "COALESCE keeps interval");
        assert!(db
            .list_enabled_scheduler_jobs()
            .await
            .expect("list")
            .is_empty());

        let run = db
            .create_sync_run(Some(&job.id), &cfg.id, "running")
            .await
            .expect("run");
        assert_eq!(
            db.delete_stale_sync_runs().await.expect("stale"),
            1,
            "running run counts as stale"
        );
        assert!(db.get_sync_run(&run.id).await.expect("get").is_none());

        assert!(db.delete_scheduler_job(&job.id).await.expect("del job"));
        assert!(db
            .delete_connector_config(&cfg.id)
            .await
            .expect("del config"));
    }
}

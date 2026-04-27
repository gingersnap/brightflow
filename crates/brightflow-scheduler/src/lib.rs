//! Brightflow Scheduler - Background job scheduling for data pipelines
//!
//! Manages recurring connector syncs backed by SQLite job definitions.

#![allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::wildcard_imports
)]

pub mod db;
pub mod error;
pub mod models;

pub use db::SchedulerDb;
pub use error::{SchedulerError, SchedulerResult};
pub use models::{ConnectorConfig, SchedulerJob, SyncRun, SyncState};

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use brightflow_connect::RunOptions;
use brightflow_core::WorkspacePaths;
use brightflow_store::ParquetStore;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod text_enrichment;

/// The scheduler reads job definitions from SQLite and manages execution.
#[derive(Clone)]
pub struct Scheduler {
    db: Arc<SchedulerDb>,
    store: Arc<ParquetStore>,
    #[allow(dead_code)]
    paths: WorkspacePaths,
    running: Arc<RwLock<HashSet<String>>>,
}

impl Scheduler {
    /// Create a new scheduler backed by SQLite
    #[must_use]
    pub fn new(db: Arc<SchedulerDb>, store: Arc<ParquetStore>, paths: WorkspacePaths) -> Self {
        Self {
            db,
            store,
            paths,
            running: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Start the scheduler background loop (ticks every 30 seconds)
    pub async fn start(&self) {
        // Clean up any stale "running" records from a previous crash/restart
        if let Err(e) = self.cleanup_stale_runs().await {
            error!("Failed to clean up stale runs: {e}");
        }
        info!("Scheduler started");
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            if let Err(e) = self.tick().await {
                error!("Scheduler tick error: {e}");
            }
        }
    }

    /// Delete any stale sync runs from a previous crash/restart
    async fn cleanup_stale_runs(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let deleted = self.db.delete_stale_sync_runs().await?;
        if deleted > 0 {
            info!("Deleted {deleted} stale sync run(s) from previous session");
        }
        Ok(())
    }

    /// Check enabled jobs and spawn any that are due
    async fn tick(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let jobs = self.db.list_enabled_scheduler_jobs().await?;

        for job in jobs {
            // Skip if already running
            if self.running.read().await.contains(&job.id) {
                continue;
            }

            // Check if job is due
            let should_run = match self.db.get_latest_sync_run_for_job(&job.id).await? {
                None => true,
                Some(last_run) => {
                    if let Ok(started) = last_run.started_at.parse::<DateTime<Utc>>() {
                        let elapsed = Utc::now().signed_duration_since(started).num_seconds();
                        elapsed >= job.interval_secs
                    } else {
                        true
                    }
                },
            };

            if should_run {
                info!("Spawning scheduled job '{}' ({})", job.name, job.id);
                self.spawn_job(&job.id, &job.connector_id, Some(&job.id))
                    .await;
            }
        }

        Ok(())
    }

    /// Trigger an immediate run for a job (ignoring schedule)
    pub async fn trigger_job(&self, job_id: &str) -> Result<String, String> {
        let job = self
            .db
            .get_scheduler_job(job_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Job not found: {job_id}"))?;

        let run_id = self
            .spawn_job(&job.id, &job.connector_id, Some(job_id))
            .await;
        Ok(run_id)
    }

    /// Trigger an immediate run for a connector (without a scheduler job)
    pub async fn trigger_connector_run(&self, connector_id: &str) -> Result<String, String> {
        let run_id = self.spawn_job("adhoc", connector_id, None).await;
        Ok(run_id)
    }

    /// Spawn a job execution in a background task
    async fn spawn_job(&self, _job_key: &str, connector_id: &str, job_id: Option<&str>) -> String {
        let db = Arc::clone(&self.db);
        let store = Arc::clone(&self.store);
        let running = Arc::clone(&self.running);
        let connector_id_owned = connector_id.to_string();
        let job_id_owned = job_id.map(ToString::to_string);
        let paths = self.paths.clone();

        // Create sync run record
        let run = match db
            .create_sync_run(job_id, &connector_id_owned, "running")
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to create sync run: {e}");
                return String::new();
            },
        };

        let run_id = run.id.clone();
        let run_id_clone = run_id.clone();

        // Mark as running
        running.write().await.insert(
            job_id_owned
                .clone()
                .unwrap_or_else(|| connector_id_owned.clone()),
        );

        tokio::spawn(async move {
            let result = execute_sync(
                &db,
                &store,
                &connector_id_owned,
                &run.id,
                job_id_owned.as_ref(),
                &paths,
            )
            .await;

            // Remove from running set
            let key = job_id_owned.unwrap_or(connector_id_owned);
            running.write().await.remove(&key);

            if let Err(e) = result {
                error!("Sync run {} failed: {e}", run.id);
                // Mark the sync run as failed in the database
                if let Err(db_err) = db
                    .update_sync_run(&run.id, "failed", None, 0, Some(&e.to_string()))
                    .await
                {
                    error!("Failed to update sync run {} as failed: {db_err}", run.id);
                }
            }
        });

        run_id_clone
    }
}

/// Execute a full sync: load config, run connector, merge results, update state
async fn execute_sync(
    db: &SchedulerDb,
    store: &ParquetStore,
    connector_id: &str,
    run_id: &str,
    _job_id: Option<&String>,
    paths: &WorkspacePaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Load connector config
    let config = db
        .get_connector_config(connector_id)
        .await?
        .ok_or_else(|| format!("Connector config not found: {connector_id}"))?;

    // 2. Load cursor values from sync_state
    let sync_states = db.list_sync_states(connector_id).await?;
    let cursor_values: HashMap<String, String> = sync_states
        .iter()
        .filter_map(|s| {
            s.cursor_value
                .as_ref()
                .map(|v| (s.endpoint.clone(), v.clone()))
        })
        .collect();

    // 3. Parse connector config JSON and expand env vars (e.g. "${GITHUB_TOKEN}")
    let raw_config: serde_json::Value = serde_json::from_str(&config.config_json)?;
    let mut config_json = substitute_env_vars_in_json(raw_config);

    // Inject token from dedicated column (takes precedence over config_json)
    if let Some(token) = &config.token {
        if let Some(obj) = config_json.as_object_mut() {
            obj.insert(
                "token".to_string(),
                serde_json::Value::String(token.clone()),
            );
        }
    }

    // Inject output_path from workspace paths — scoped by preset id so concurrent
    // runs of two presets of the same connector type don't race on the same file.
    let connector_output_dir = paths.connector_output_for_preset(&config.id);
    std::fs::create_dir_all(&connector_output_dir)?;
    if let Some(obj) = config_json.as_object_mut() {
        obj.insert(
            "output_path".to_string(),
            serde_json::Value::String(connector_output_dir.display().to_string()),
        );
    }

    // 4. Build run options with cursors
    let options = RunOptions {
        only: None,
        dry_run: false,
        cursor_values,
    };

    // 5. Run the connector — use embedded source for builtins, filesystem path otherwise
    let result = if let Some(lua_source) =
        brightflow_connect::get_builtin_connector_source(&config.connector_path)
    {
        brightflow_connect::run_connector_from_source(lua_source, config_json, &options)
            .await
            .map_err(|e| format!("Connector execution failed: {e}"))?
    } else {
        let connector_path = PathBuf::from(&config.connector_path);
        brightflow_connect::run_connector_with_config(&connector_path, config_json, &options)
            .await
            .map_err(|e| format!("Connector execution failed: {e}"))?
    };

    // 7. For each endpoint result, merge parquet into table
    let output_dir = PathBuf::from(&result.output_path);
    let mut total_rows: i64 = 0;

    for ep_result in &result.endpoints {
        let parquet_file = output_dir.join(format!("{}.parquet", ep_result.name));
        if !parquet_file.exists() {
            warn!(
                "Expected parquet file not found: {}",
                parquet_file.display()
            );
            continue;
        }

        let source_id = format!("connector:{connector_id}");

        // Enrich with text-derived columns if applicable (issues today; PRs/comments later)
        let workspace_root = paths.root();
        if let Err(e) = text_enrichment::maybe_enrich_parquet(
            &parquet_file,
            &ep_result.name,
            &source_id,
            &workspace_root,
        ) {
            warn!("Text enrichment skipped for {}: {e}", ep_result.name);
        }

        let metrics = store
            .merge_parquet(
                &source_id,
                &ep_result.name,
                &parquet_file,
                &ep_result.primary_key,
            )
            .await
            .map_err(|e| format!("Merge failed for {}: {e}", ep_result.name))?;

        let rows = i64::try_from(metrics.rows_inserted + metrics.rows_updated).unwrap_or(i64::MAX);
        total_rows += rows;

        info!(
            "Merged {}: {} inserted, {} updated",
            ep_result.name, metrics.rows_inserted, metrics.rows_updated
        );

        // 8. Update sync_state with cursor from result
        db.upsert_sync_state(
            connector_id,
            &ep_result.name,
            ep_result.cursor_field.as_deref(),
            ep_result.cursor_value.as_deref(),
            "success",
            rows,
        )
        .await?;
    }

    // 9. Update sync run as completed
    let endpoint_names: Vec<&str> = result.endpoints.iter().map(|e| e.name.as_str()).collect();
    let endpoints_json = serde_json::to_string(&endpoint_names)?;
    db.update_sync_run(run_id, "completed", Some(&endpoints_json), total_rows, None)
        .await?;

    info!("Sync run {run_id} completed: {total_rows} total rows");

    Ok(())
}

/// Recursively expand `${ENV_VAR}` patterns in JSON string values
fn substitute_env_vars_in_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            if s.contains("${") {
                let expanded = substitute_env_vars_str(&s);
                serde_json::Value::String(expanded)
            } else {
                serde_json::Value::String(s)
            }
        },
        serde_json::Value::Object(map) => {
            let expanded = map
                .into_iter()
                .map(|(k, v)| (k, substitute_env_vars_in_json(v)))
                .collect();
            serde_json::Value::Object(expanded)
        },
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(substitute_env_vars_in_json).collect())
        },
        other => other,
    }
}

fn substitute_env_vars_str(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

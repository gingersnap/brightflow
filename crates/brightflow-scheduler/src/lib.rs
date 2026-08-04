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

/// Hook invoked after each endpoint's merge, with `(source_id, table_name)`.
///
/// Failures must be handled inside the hook — they never fail the sync. The
/// API installs one to trigger incremental LLM-enrichment runs; the
/// scheduler itself gains no LLM dependency.
pub type PostSyncHook = Arc<
    dyn Fn(String, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// The scheduler reads job definitions from SQLite and manages execution.
#[derive(Clone)]
pub struct Scheduler {
    db: Arc<SchedulerDb>,
    store: Arc<ParquetStore>,
    paths: WorkspacePaths,
    running: Arc<RwLock<HashSet<String>>>,
    post_sync_hook: Arc<RwLock<Option<PostSyncHook>>>,
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
            post_sync_hook: Arc::new(RwLock::new(None)),
        }
    }

    /// Install the post-sync hook (called once at API startup).
    pub async fn set_post_sync_hook(&self, hook: PostSyncHook) {
        *self.post_sync_hook.write().await = Some(hook);
    }

    /// Start the scheduler background loop (ticks every 30 seconds)
    pub async fn start(&self) {
        // Clean up any stale "running" records from a previous crash/restart
        if let Err(e) = self.cleanup_stale_runs().await {
            error!("Failed to clean up stale runs: {e}");
        }
        info!("Scheduler started");
        // interval_at keeps the wait-one-period-first startup behavior of the
        // old sleep loop; Delay keeps ticks spaced a full period after a slow
        // tick instead of bursting to catch up.
        let period = tokio::time::Duration::from_secs(30);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
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
        let hook = self.post_sync_hook.read().await.clone();

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
                hook.as_ref(),
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

/// Effective topic-model config for a table at sync time.
///
/// A promoted `topic_model` function makes ANY table enrichable (its config
/// resolves builtin defaults ⊕ overrides, or a plain-profile base for
/// arbitrary tables). With no promoted function, builtin-default tables keep
/// enriching as before; everything else skips.
async fn resolve_topic_config(
    store: &ParquetStore,
    source_id: &str,
    table_name: &str,
) -> Option<brightflow_engine::enrichment::EnrichmentConfig> {
    use brightflow_engine::enrichment::{EnrichmentConfig, FunctionSpec};
    if let Ok(Some(table)) = store.db().get_table(source_id, table_name).await {
        if let Ok(functions) = store
            .db()
            .list_promoted_functions(&table.id, "topic_model")
            .await
        {
            if let Some(function) = functions.first() {
                if let Ok(Some(version)) = store
                    .db()
                    .get_enrichment_function_version(&function.id, function.current_version)
                    .await
                {
                    if let Ok(FunctionSpec::TopicModel(tm)) =
                        serde_json::from_str::<FunctionSpec>(&version.config_json)
                    {
                        return Some(tm.to_config(table_name));
                    }
                    warn!(
                        "topic_model function {} has unreadable config — falling back to builtin",
                        function.id
                    );
                }
            }
        }
    }
    EnrichmentConfig::builtin_default(table_name)
}

/// Execute a full sync: load config, run connector, merge results, update state
async fn execute_sync(
    db: &SchedulerDb,
    store: &ParquetStore,
    connector_id: &str,
    run_id: &str,
    _job_id: Option<&String>,
    paths: &WorkspacePaths,
    post_sync_hook: Option<&PostSyncHook>,
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

        // Enrich with text-derived columns when a promoted topic_model
        // function (or a builtin default) applies.
        let workspace_root = paths.root();
        let enrichment_config = resolve_topic_config(store, &source_id, &ep_result.name).await;
        if let Err(e) = text_enrichment::maybe_enrich_parquet(
            &parquet_file,
            &ep_result.name,
            &source_id,
            &workspace_root,
            enrichment_config.as_ref(),
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

        // Post-sync hook (e.g. incremental LLM enrichment). The hook handles
        // its own failures; it never fails the sync.
        if let Some(hook) = post_sync_hook {
            hook(source_id.clone(), ep_result.name.clone()).await;
        }
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

/// Expand `${VAR}` occurrences in `s` using `lookup`, scanning left to right.
///
/// The cursor only moves forward, so substituted values are never re-scanned.
/// That is deliberate on two counts: env content is data, not template source,
/// and a self-referential variable (`FOO=${FOO}`) terminates instead of
/// growing the string forever — a rescan-from-zero loop hangs the scheduler
/// mid-sync with nobody watching.
///
/// Unresolved variables expand to the empty string; a `${` with no closing
/// brace is passed through untouched.
fn substitute_with(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cursor = 0;

    while let Some(rel_start) = s[cursor..].find("${") {
        let start = cursor + rel_start;
        let Some(rel_end) = s[start..].find('}') else {
            break;
        };
        let end = start + rel_end;

        out.push_str(&s[cursor..start]);
        out.push_str(&lookup(&s[start + 2..end]).unwrap_or_default());
        cursor = end + 1;
    }

    out.push_str(&s[cursor..]);
    out
}

/// Expand `${ENV_VAR}` patterns in a string against the process environment.
fn substitute_env_vars_str(s: &str) -> String {
    substitute_with(s, |key| std::env::var(key).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lookup over a fixed table, so tests never touch the process env.
    fn table(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|&(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn string_without_placeholder_is_unchanged() {
        assert_eq!(substitute_with("plain text", table(&[])), "plain text");
    }

    #[test]
    fn single_placeholder_is_replaced() {
        let out = substitute_with("Bearer ${TOKEN}", table(&[("TOKEN", "abc")]));
        assert_eq!(out, "Bearer abc");
    }

    #[test]
    fn adjacent_placeholders_both_resolve() {
        // Pins the offset arithmetic: the second `${` must be found relative to
        // the input, not the partially built output.
        let out = substitute_with("${A}${B}", table(&[("A", "1"), ("B", "2")]));
        assert_eq!(out, "12");
    }

    #[test]
    fn missing_variable_expands_to_empty_string() {
        assert_eq!(substitute_with("x=${NOPE};", table(&[])), "x=;");
    }

    #[test]
    fn unterminated_placeholder_passes_through() {
        assert_eq!(substitute_with("${nope", table(&[])), "${nope");
    }

    #[test]
    fn self_referential_value_terminates_and_stays_literal() {
        // Regression: rescanning from position 0 re-expanded substituted values,
        // so this input looped forever.
        let out = substitute_with("${FOO}", table(&[("FOO", "${FOO}")]));
        assert_eq!(out, "${FOO}");
    }

    #[test]
    fn json_substitution_only_touches_string_leaves() {
        // Uses a name no environment plausibly sets, so the walk is asserted
        // against the deterministic "unresolved" branch without mutating the
        // process env (which would race with the parallel test harness).
        let input = serde_json::json!({
            "token": "${BRIGHTFLOW_TEST_UNSET_VAR}",
            "port": 8080,
            "enabled": true,
            "missing": null,
            "nested": {
                "headers": ["${BRIGHTFLOW_TEST_UNSET_VAR}", "static"],
            },
        });

        let out = substitute_env_vars_in_json(input);

        assert_eq!(out["token"], serde_json::json!(""));
        assert_eq!(out["port"], serde_json::json!(8080));
        assert_eq!(out["enabled"], serde_json::json!(true));
        assert_eq!(out["missing"], serde_json::Value::Null);
        assert_eq!(out["nested"]["headers"][0], serde_json::json!(""));
        assert_eq!(out["nested"]["headers"][1], serde_json::json!("static"));
    }
}

//! Everything that runs in the background, as one list with one shape.
//!
//! Connector syncs, enrichment runs, agent runs and insight runs each keep
//! their own table with their own columns and clocks. The Activity page
//! needs them as one thing: what is running now, and what finished when,
//! next to the actions it caused. This module reads the four tables and
//! renders each row as a `Job` with a common status, epoch timestamps and
//! a one-line detail. Cancelling stays with each kind's own endpoint; a job
//! says whether it can be cancelled and the client knows where.
//!
//! The same shape rides the socket as a `job` frame for the two kinds that
//! had no frame of their own: syncs, through the scheduler's listener, and
//! enrichment runs, at start, every progress write and finish.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use brightflow_scheduler::SyncRun;
use brightflow_store::RecentEnrichmentRunRow;

use crate::shared::AppResult;
use crate::state::AppState;

/// Rows read per kind before the merge; the client asks for fewer.
const PER_KIND: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ConnectorSync,
    EnrichmentRun,
    AgentRun,
    InsightRun,
    ModelBuild,
}

/// One background run as the Activity page shows it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// The run's own id, unique within its kind.
    pub id: String,
    pub kind: JobKind,
    /// "GitHub sync", "Classify tickets", "Describe table", "Insights".
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub table: Option<String>,
    /// running | completed | failed | cancelled
    pub status: String,
    /// Unix seconds.
    #[ts(type = "number")]
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub finished_at: Option<i64>,
    /// One line: rows synced, rows done of total, proposals, findings, or the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    /// Whether the kind's cancel endpoint accepts this job while it runs.
    pub cancellable: bool,
}

#[derive(Debug, Deserialize)]
pub struct JobsQuery {
    pub limit: Option<usize>,
}

/// Unix seconds from the two clocks the tables use: RFC 3339 (the
/// scheduler) and SQLite's `datetime('now')`, "YYYY-MM-DD HH:MM:SS" in UTC
/// (the store). `None` for anything else.
pub fn epoch_of(text: &str) -> Option<i64> {
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(t.timestamp());
    }
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|t| t.and_utc().timestamp())
}

/// The (source, table) an agent run's scope key names.
///
/// The key is `{kind}:{source}:{table}` with `:{parent}` appended for
/// subcategory runs, and a source id may itself contain colons, so the
/// table is the last segment once the kind and any parent are removed.
pub fn agent_scope(kind: &str, scope: &str) -> Option<(String, String)> {
    let rest = scope.strip_prefix(kind)?.strip_prefix(':')?;
    let rest = if kind == "propose_subcategories" {
        rest.rsplit_once(':')
            .filter(|(_, parent)| parent.chars().all(|c| c.is_ascii_digit()))
            .map_or(rest, |(head, _)| head)
    } else {
        rest
    };
    let (source, table) = rest.rsplit_once(':')?;
    if source.is_empty() || table.is_empty() {
        return None;
    }
    Some((source.to_string(), table.to_string()))
}

/// The run kind as a person reads it.
pub fn agent_label(kind: &str) -> String {
    match kind {
        "describe_table" => "Describe table".to_string(),
        "triage_insights" => "Triage insights".to_string(),
        "narrate_insights" => "Summarize insights".to_string(),
        "propose_categories" => "Propose categories".to_string(),
        "propose_subcategories" => "Propose subcategories".to_string(),
        "propose_feedback_categories" => "Propose feedback categories".to_string(),
        other => other.replace('_', " "),
    }
}

/// Newest first by start time; a running job never sorts below a finished
/// one that started at the same second.
pub fn sort_jobs(jobs: &mut [Job]) {
    jobs.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| (b.status == "running").cmp(&(a.status == "running")))
    });
}

/// A sync run as a job. The connector's name is looked up by the caller;
/// the row only carries its id.
pub fn sync_job(r: SyncRun, connector_name: &str) -> Job {
    let detail = match (&r.error, r.status.as_str()) {
        (Some(e), _) => Some(e.clone()),
        (None, "running") => None,
        (None, _) => Some(format!("{} rows synced", r.rows_synced)),
    };
    Job {
        id: r.id,
        kind: JobKind::ConnectorSync,
        label: format!("{connector_name} sync"),
        source_id: Some(format!("connector:{connector_name}")),
        table: None,
        status: r.status,
        started_at: epoch_of(&r.started_at).unwrap_or(0),
        finished_at: r.finished_at.as_deref().and_then(epoch_of),
        detail,
        cancellable: false,
    }
}

/// A model build as a job. `table` is the model's output.
pub fn model_job(b: &brightflow_store::ModelBuildRow, source_id: &str, table: &str) -> Job {
    let detail = match (&b.error, b.rows) {
        (Some(e), _) => Some(e.clone()),
        (None, Some(rows)) => Some(format!("{rows} rows")),
        (None, None) => None,
    };
    Job {
        id: b.id.clone(),
        kind: JobKind::ModelBuild,
        label: format!("Build {table} ({})", b.triggered_by),
        source_id: Some(source_id.to_string()),
        table: Some(table.to_string()),
        status: b.status.clone(),
        started_at: b.started_at,
        finished_at: b.finished_at,
        detail,
        cancellable: false,
    }
}

/// Push a model build's current row to every client.
pub async fn emit_model_job(state: &AppState, build_id: &str) {
    let Some(store) = state.store() else { return };
    let db = store.db();
    let Ok(Some(build)) = db.get_model_build(build_id).await else {
        return;
    };
    let Ok(Some(model)) = db.get_model(&build.model_id).await else {
        return;
    };
    let Ok(Some(table)) = db.get_table_by_id(&model.output_table_id).await else {
        return;
    };
    crate::actions::events::emit_job(state, model_job(&build, &table.source_id, &table.name));
}

/// An enrichment run as a job.
pub fn enrichment_job(r: RecentEnrichmentRunRow) -> Job {
    let detail = match (&r.error, r.status.as_str()) {
        (Some(e), _) => Some(e.clone()),
        (None, "running") => Some(format!("{} of {} rows", r.rows_done, r.rows_total)),
        (None, _) => Some(format!(
            "{} rows{}",
            r.rows_done,
            if r.rows_failed > 0 {
                format!(", {} failed", r.rows_failed)
            } else {
                String::new()
            }
        )),
    };
    Job {
        id: r.id,
        kind: JobKind::EnrichmentRun,
        label: format!("{} ({})", r.function_name, r.mode),
        source_id: Some(r.source_id),
        table: Some(r.table_name),
        cancellable: r.status == "running",
        status: r.status,
        started_at: epoch_of(&r.created_at).unwrap_or(0),
        finished_at: r.finished_at.as_deref().and_then(epoch_of),
        detail,
    }
}

/// Push an enrichment run's current row to every client, with the names
/// the row does not carry looked up. Best effort: a run the store cannot
/// find emits nothing.
pub async fn emit_enrichment_job(state: &AppState, run_id: &str) {
    let Some(store) = state.store() else { return };
    let db = store.db();
    let Ok(Some(run)) = db.get_enrichment_run(run_id).await else {
        return;
    };
    let Ok(Some(function)) = db.get_enrichment_function(&run.function_id).await else {
        return;
    };
    let Ok(Some(table)) = db.get_table_by_id(&function.table_id).await else {
        return;
    };
    let row = RecentEnrichmentRunRow {
        id: run.id,
        function_id: run.function_id,
        function_name: function.name,
        source_id: table.source_id,
        table_name: table.name,
        mode: run.mode,
        status: run.status,
        rows_total: run.rows_total,
        rows_done: run.rows_done,
        rows_failed: run.rows_failed,
        error: run.error,
        created_at: run.created_at,
        finished_at: run.finished_at,
    };
    crate::actions::events::emit_job(state, enrichment_job(row));
}

/// Push a sync run to every client. The scheduler's listener calls this
/// on its own task, since the listener itself must not block the sync.
pub async fn emit_sync_job(state: AppState, run: SyncRun) {
    let name = match state.scheduler_db.as_ref() {
        Some(db) => db
            .list_connector_configs()
            .await
            .ok()
            .and_then(|configs| {
                configs
                    .into_iter()
                    .find(|c| c.id == run.connector_id)
                    .map(|c| c.name)
            })
            .unwrap_or_else(|| run.connector_id.clone()),
        None => run.connector_id.clone(),
    };
    crate::actions::events::emit_job(&state, sync_job(run, &name));
}

async fn sync_jobs(state: &AppState) -> Vec<Job> {
    let Some(db) = state.scheduler_db.as_ref() else {
        return Vec::new();
    };
    let (Ok(runs), Ok(configs)) = (
        db.list_sync_runs(PER_KIND).await,
        db.list_connector_configs().await,
    ) else {
        return Vec::new();
    };
    runs.into_iter()
        .map(|r| {
            let name = configs
                .iter()
                .find(|c| c.id == r.connector_id)
                .map_or_else(|| r.connector_id.clone(), |c| c.name.clone());
            sync_job(r, &name)
        })
        .collect()
}

async fn enrichment_jobs(state: &AppState) -> Vec<Job> {
    let Some(store) = state.store() else {
        return Vec::new();
    };
    let Ok(runs) = store.db().list_recent_enrichment_runs(PER_KIND).await else {
        return Vec::new();
    };
    runs.into_iter().map(enrichment_job).collect()
}

async fn agent_jobs(state: &AppState) -> Vec<Job> {
    let Some(store) = state.store() else {
        return Vec::new();
    };
    let Ok(runs) = store.db().list_agent_runs(PER_KIND).await else {
        return Vec::new();
    };
    runs.into_iter()
        .map(|r| {
            let (source_id, table) =
                agent_scope(&r.kind, &r.scope).map_or((None, None), |(s, t)| (Some(s), Some(t)));
            // The first line of the detail is the stats; the rest is the
            // model's note, which the run card shows in full.
            let detail = r
                .detail
                .as_deref()
                .and_then(|d| d.lines().next())
                .map(str::to_string);
            Job {
                id: r.id.to_string(),
                kind: JobKind::AgentRun,
                label: agent_label(&r.kind),
                source_id,
                table,
                cancellable: r.status == "running",
                status: r.status,
                started_at: r.created_at,
                finished_at: r.finished_at,
                detail,
            }
        })
        .collect()
}

/// Model builds, each named by its output table.
async fn model_jobs(state: &AppState) -> Vec<Job> {
    let Some(store) = state.store() else {
        return Vec::new();
    };
    let db = store.db();
    let Ok(builds) = db.list_recent_model_builds(PER_KIND).await else {
        return Vec::new();
    };
    let mut jobs = Vec::with_capacity(builds.len());
    for build in &builds {
        let Ok(Some(model)) = db.get_model(&build.model_id).await else {
            continue;
        };
        let Ok(Some(table)) = db.get_table_by_id(&model.output_table_id).await else {
            continue;
        };
        jobs.push(model_job(build, &table.source_id, &table.name));
    }
    jobs
}

async fn insight_jobs(state: &AppState) -> Vec<Job> {
    let Some(store) = state.store() else {
        return Vec::new();
    };
    let Ok(runs) = store.db().list_recent_insight_runs(PER_KIND).await else {
        return Vec::new();
    };
    runs.into_iter()
        .map(|r| {
            // An insight run is recorded when it finishes: it is never
            // "running" here, and its duration is what it took.
            #[allow(clippy::cast_possible_truncation)]
            let started = r.computed_at - (r.execution_time_ms / 1000.0).round() as i64;
            Job {
                id: r.id.to_string(),
                kind: JobKind::InsightRun,
                label: if r.triggered_by == "post_sync" {
                    "Insights after sync".to_string()
                } else {
                    "Insights".to_string()
                },
                source_id: Some(r.source_id),
                table: Some(r.table_name),
                status: "completed".to_string(),
                started_at: started,
                finished_at: Some(r.computed_at),
                detail: Some(format!(
                    "{} findings, {} new",
                    r.finding_count, r.new_finding_count
                )),
                cancellable: false,
            }
        })
        .collect()
}

/// `GET /api/jobs?limit=` — every kind of background run, newest first.
pub async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<JobsQuery>,
) -> AppResult<Json<Vec<Job>>> {
    let limit = query.limit.unwrap_or(50).clamp(1, 400);
    let mut jobs = Vec::new();
    jobs.extend(sync_jobs(&state).await);
    jobs.extend(enrichment_jobs(&state).await);
    jobs.extend(agent_jobs(&state).await);
    jobs.extend(insight_jobs(&state).await);
    jobs.extend(model_jobs(&state).await);
    sort_jobs(&mut jobs);
    jobs.truncate(limit);
    Ok(Json(jobs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_of_reads_both_clocks() {
        assert_eq!(epoch_of("2026-09-13T10:00:00+00:00"), Some(1_789_293_600));
        assert_eq!(epoch_of("2026-09-13 10:00:00"), Some(1_789_293_600));
        assert_eq!(epoch_of("2026-09-13T12:00:00+02:00"), Some(1_789_293_600));
        assert_eq!(epoch_of("yesterday"), None);
    }

    #[test]
    fn agent_scope_handles_colons_in_source_ids_and_a_parent_suffix() {
        assert_eq!(
            agent_scope("describe_table", "describe_table:connector:sample:issues"),
            Some(("connector:sample".to_string(), "issues".to_string()))
        );
        assert_eq!(
            agent_scope(
                "propose_subcategories",
                "propose_subcategories:src:issues:42"
            ),
            Some(("src".to_string(), "issues".to_string()))
        );
        assert_eq!(agent_scope("describe_table", "other:src:issues"), None);
        assert_eq!(agent_scope("describe_table", "describe_table:issues"), None);
    }

    fn job(started_at: i64, status: &str) -> Job {
        Job {
            id: String::new(),
            kind: JobKind::AgentRun,
            label: String::new(),
            source_id: None,
            table: None,
            status: status.to_string(),
            started_at,
            finished_at: None,
            detail: None,
            cancellable: false,
        }
    }

    #[test]
    fn sort_jobs_is_newest_first_with_running_ahead_on_ties() {
        let mut jobs = vec![
            job(5, "completed"),
            job(9, "completed"),
            job(9, "running"),
            job(1, "failed"),
        ];
        sort_jobs(&mut jobs);
        let order: Vec<(i64, &str)> = jobs
            .iter()
            .map(|j| (j.started_at, j.status.as_str()))
            .collect();
        assert_eq!(
            order,
            vec![
                (9, "running"),
                (9, "completed"),
                (5, "completed"),
                (1, "failed")
            ]
        );
    }

    #[test]
    fn sync_job_names_the_connector_and_phrases_the_outcome() {
        let running = SyncRun {
            id: "r1".to_string(),
            job_id: None,
            connector_id: "cfg-1".to_string(),
            started_at: "2026-09-13T10:00:00+00:00".to_string(),
            finished_at: None,
            status: "running".to_string(),
            endpoints_synced: None,
            rows_synced: 0,
            error: None,
        };
        let job = sync_job(running.clone(), "github");
        assert_eq!(job.label, "github sync");
        assert_eq!(job.source_id.as_deref(), Some("connector:github"));
        assert_eq!(job.started_at, 1_789_293_600);
        assert_eq!(job.detail, None);
        assert!(!job.cancellable);

        let done = SyncRun {
            status: "completed".to_string(),
            finished_at: Some("2026-09-13T10:05:00+00:00".to_string()),
            rows_synced: 1203,
            ..running.clone()
        };
        assert_eq!(
            sync_job(done, "github").detail.as_deref(),
            Some("1203 rows synced")
        );
        let failed = SyncRun {
            status: "failed".to_string(),
            error: Some("401 from api.github.com".to_string()),
            ..running
        };
        assert_eq!(
            sync_job(failed, "github").detail.as_deref(),
            Some("401 from api.github.com")
        );
    }

    #[test]
    fn enrichment_job_reports_progress_while_running_and_totals_after() {
        let row = |status: &str, done: i64, failed: i64| RecentEnrichmentRunRow {
            id: "e1".to_string(),
            function_id: "f1".to_string(),
            function_name: "Classify tickets".to_string(),
            source_id: "connector:github".to_string(),
            table_name: "issues".to_string(),
            mode: "full".to_string(),
            status: status.to_string(),
            rows_total: 40,
            rows_done: done,
            rows_failed: failed,
            error: None,
            created_at: "2026-09-13 10:00:00".to_string(),
            finished_at: None,
        };
        let live = enrichment_job(row("running", 12, 0));
        assert_eq!(live.label, "Classify tickets (full)");
        assert_eq!(live.detail.as_deref(), Some("12 of 40 rows"));
        assert!(live.cancellable);
        assert_eq!(live.table.as_deref(), Some("issues"));
        let done = enrichment_job(row("completed", 40, 3));
        assert_eq!(done.detail.as_deref(), Some("40 rows, 3 failed"));
        assert!(!done.cancellable);
    }

    #[test]
    fn agent_label_reads_as_prose() {
        assert_eq!(agent_label("describe_table"), "Describe table");
        assert_eq!(agent_label("some_new_kind"), "some new kind");
    }
}

//! Everything that runs in the background, as one list with one shape.
//!
//! Connector syncs, enrichment runs, agent runs and insight runs each keep
//! their own table with their own columns and clocks. The Activity page
//! needs them as one thing: what is running now, and what finished when,
//! next to the actions it caused. This module reads the four tables and
//! renders each row as a `Job` with a common status, epoch timestamps and
//! a one-line detail. Cancelling stays with each kind's own endpoint; a job
//! says whether it can be cancelled and the client knows where.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
            let detail = match (&r.error, r.status.as_str()) {
                (Some(e), _) => Some(e.clone()),
                (None, "running") => None,
                (None, _) => Some(format!("{} rows synced", r.rows_synced)),
            };
            Job {
                id: r.id,
                kind: JobKind::ConnectorSync,
                label: format!("{name} sync"),
                source_id: Some(format!("connector:{name}")),
                table: None,
                status: r.status,
                started_at: epoch_of(&r.started_at).unwrap_or(0),
                finished_at: r.finished_at.as_deref().and_then(epoch_of),
                detail,
                cancellable: false,
            }
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
    runs.into_iter()
        .map(|r| {
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
        })
        .collect()
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
    fn agent_label_reads_as_prose() {
        assert_eq!(agent_label("describe_table"), "Describe table");
        assert_eq!(agent_label("some_new_kind"), "some new kind");
    }
}

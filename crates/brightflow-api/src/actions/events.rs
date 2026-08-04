//! Live curation events: typed payloads pushed to every `/api/ws` client.
//!
//! Single-tenant assumption: there is one workspace, so every connected
//! client gets every event — no per-tenant or per-scope filtering.
//!
//! All emits are best-effort: a `send` error just means nobody is connected.

use serde::Serialize;
use ts_rs::TS;

use crate::actions::types::ActionLogEntry;
use crate::agent::types::AgentRunResponse;
use crate::state::AppState;

/// Cap on entries carried by one batch event. Bigger batches set `truncated`
/// and the client refetches the feed instead of replaying entries.
pub const BATCH_EVENT_CAP: usize = 100;

/// An event on the curation bus. Fanned out verbatim to WS clients as
/// `WsServerMessage` variants.
#[derive(Debug, Clone)]
pub enum CurationEvent {
    Action(ActionEventPayload),
    ActionBatch(ActionBatchPayload),
    AgentRun(AgentRunEventPayload),
    InsightsComputed(InsightsComputedPayload),
}

/// One action-log row changed (created, applied, failed, rejected, undone).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionEventPayload {
    pub entry: ActionLogEntry,
    /// Server-computed proposals-awaiting-review count, so the client never
    /// has to delta-account the badge.
    pub pending_count: usize,
}

/// Many rows changed at once (approve-all, undo-all).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ActionBatchPayload {
    /// Newest `BATCH_EVENT_CAP` entries of the batch.
    pub entries: Vec<ActionLogEntry>,
    /// True when the batch exceeded the cap — refetch the feed instead.
    pub truncated: bool,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub pending_count: usize,
}

/// An agent run started, finished, failed, or was cancelled.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunEventPayload {
    pub run: AgentRunResponse,
}

/// An insights run (manual or post-sync) finished — powers the sidebar badge.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InsightsComputedPayload {
    #[ts(type = "number")]
    pub run_id: i64,
    pub source_id: String,
    pub table: String,
    pub report_type: String,
    /// "manual" | "post_sync"
    pub triggered_by: String,
    #[ts(type = "number")]
    pub finding_count: i64,
    #[ts(type = "number")]
    pub new_finding_count: i64,
    #[ts(optional)]
    pub top_summary: Option<String>,
    /// Unix epoch seconds
    #[ts(type = "number")]
    pub computed_at: i64,
}

/// Current proposals-awaiting-review count (0 on any failure — best effort).
pub async fn pending_count(state: &AppState) -> usize {
    let Some(store) = state.store() else { return 0 };
    store
        .db()
        .count_proposed_actions()
        .await
        .ok()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0)
}

/// Emit a single action-log change.
pub async fn emit_action(state: &AppState, entry: ActionLogEntry) {
    let payload = ActionEventPayload {
        entry,
        pending_count: pending_count(state).await,
    };
    state
        .curation_events
        .send(CurationEvent::Action(payload))
        .ok();
}

/// Emit one batch event for a bulk operation. `entries` should arrive in
/// application order; only the newest `BATCH_EVENT_CAP` ride the event.
pub async fn emit_batch(
    state: &AppState,
    mut entries: Vec<ActionLogEntry>,
    total: usize,
    succeeded: usize,
    failed: usize,
) {
    let truncated = entries.len() > BATCH_EVENT_CAP;
    if truncated {
        entries.drain(..entries.len() - BATCH_EVENT_CAP);
    }
    let payload = ActionBatchPayload {
        entries,
        truncated,
        total,
        succeeded,
        failed,
        pending_count: pending_count(state).await,
    };
    state
        .curation_events
        .send(CurationEvent::ActionBatch(payload))
        .ok();
}

/// Emit a finished insights run (badge + activity surfaces).
pub fn emit_insights_computed(state: &AppState, row: &brightflow_store::InsightRunRow) {
    let payload = InsightsComputedPayload {
        run_id: row.id,
        source_id: row.source_id.clone(),
        table: row.table_name.clone(),
        report_type: row.report_type.clone(),
        triggered_by: row.triggered_by.clone(),
        finding_count: row.finding_count,
        new_finding_count: row.new_finding_count,
        top_summary: row.top_summary.clone(),
        computed_at: row.computed_at,
    };
    state
        .curation_events
        .send(CurationEvent::InsightsComputed(payload))
        .ok();
}

/// Emit the current state of an agent run (fetched fresh).
pub async fn emit_run(state: &AppState, run_id: i64) {
    let Some(store) = state.store() else { return };
    let Ok(Some(row)) = store.db().get_agent_run(run_id).await else {
        return;
    };
    let actions = store
        .db()
        .list_actions_for_agent_run(run_id)
        .await
        .map(|rows| rows.into_iter().map(|a| a.id).collect())
        .unwrap_or_default();
    emit_run_row(state, row, actions);
}

/// Emit an agent-run row already in hand (no re-fetch).
pub fn emit_run_row(state: &AppState, row: brightflow_store::AgentRunRow, actions: Vec<i64>) {
    let payload = AgentRunEventPayload {
        run: crate::agent::handlers::to_response(row, actions),
    };
    state
        .curation_events
        .send(CurationEvent::AgentRun(payload))
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::types::WsServerMessage;

    fn entry() -> ActionLogEntry {
        ActionLogEntry::from_row(brightflow_store::ActionLogRow {
            id: 7,
            request_id: "req-1".to_string(),
            actor_type: "human".to_string(),
            agent_run_id: None,
            action_kind: "rename_cluster".to_string(),
            params_json: r#"{"kind":"rename_cluster"}"#.to_string(),
            result_json: None,
            undo_json: Some("{}".to_string()),
            status: "applied".to_string(),
            created_at: 1,
            resolved_at: Some(2),
        })
    }

    /// The WS wire format is the frontend contract — pin the tags and casing.
    #[test]
    fn action_event_wire_format() {
        let msg = WsServerMessage::from(CurationEvent::Action(ActionEventPayload {
            entry: entry(),
            pending_count: 3,
        }));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"actionEvent""#), "{json}");
        assert!(json.contains(r#""pendingCount":3"#), "{json}");
        assert!(json.contains(r#""actionKind":"rename_cluster""#), "{json}");
        assert!(json.contains(r#""undoable":true"#), "{json}");
    }

    #[test]
    fn action_batch_wire_format() {
        let msg = WsServerMessage::from(CurationEvent::ActionBatch(ActionBatchPayload {
            entries: vec![entry()],
            truncated: false,
            total: 1,
            succeeded: 1,
            failed: 0,
            pending_count: 0,
        }));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"actionBatch""#), "{json}");
        assert!(json.contains(r#""truncated":false"#), "{json}");
        assert!(json.contains(r#""succeeded":1"#), "{json}");
    }

    #[test]
    fn resync_wire_format() {
        let json = serde_json::to_string(&WsServerMessage::ActionResync).unwrap();
        assert_eq!(json, r#"{"type":"actionResync"}"#);
    }

    #[test]
    fn agent_run_wire_format() {
        let msg = WsServerMessage::from(CurationEvent::AgentRun(AgentRunEventPayload {
            run: crate::agent::handlers::to_response(
                brightflow_store::AgentRunRow {
                    id: 4,
                    kind: "auto_label".to_string(),
                    mode: "propose".to_string(),
                    scope: "auto_label:s:t".to_string(),
                    status: "running".to_string(),
                    detail: None,
                    created_at: 1,
                    finished_at: None,
                },
                Vec::new(),
            ),
        }));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"agentRun""#), "{json}");
        assert!(json.contains(r#""status":"running""#), "{json}");
    }

    /// The `insightsComputed` frame is the frontend badge contract — pin it.
    #[test]
    fn insights_computed_wire_format() {
        let msg = WsServerMessage::from(CurationEvent::InsightsComputed(InsightsComputedPayload {
            run_id: 12,
            source_id: "s1".to_string(),
            table: "issues".to_string(),
            report_type: "trends".to_string(),
            triggered_by: "post_sync".to_string(),
            finding_count: 7,
            new_finding_count: 2,
            top_summary: Some("Comments spiked".to_string()),
            computed_at: 99,
        }));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"insightsComputed""#), "{json}");
        assert!(json.contains(r#""newFindingCount":2"#), "{json}");
        assert!(json.contains(r#""triggeredBy":"post_sync""#), "{json}");
        assert!(json.contains(r#""sourceId":"s1""#), "{json}");
    }

    /// Oversized batches keep the NEWEST entries and set `truncated`.
    #[tokio::test]
    async fn emit_batch_caps_to_newest() {
        let state = AppState::new();
        let mut rx = state.curation_events.subscribe();
        let cap = i64::try_from(BATCH_EVENT_CAP).unwrap();
        let entries: Vec<ActionLogEntry> = (0..(cap + 5))
            .map(|i| {
                let mut e = entry();
                e.id = i;
                e
            })
            .collect();
        emit_batch(&state, entries, 105, 105, 0).await;
        let Ok(CurationEvent::ActionBatch(p)) = rx.try_recv() else {
            panic!("expected batch event");
        };
        assert!(p.truncated);
        assert_eq!(p.entries.len(), BATCH_EVENT_CAP);
        assert_eq!(p.entries.first().map(|e| e.id), Some(5));
        assert_eq!(p.entries.last().map(|e| e.id), Some(cap + 4));
    }
}

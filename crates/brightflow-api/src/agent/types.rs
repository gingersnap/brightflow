use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Body for `POST /api/agent/runs`.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StartAgentRunRequest {
    /// "auto_label" | "propose_merges" | "narrate_insights" | "triage_insights"
    /// | "propose_taxonomy" | "label_documents"
    pub kind: String,
    pub source_id: String,
    pub table: String,
}

/// One agent run, as returned by the API.
#[derive(Debug, Serialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResponse {
    pub id: i64,
    pub kind: String,
    pub mode: String,
    pub scope: String,
    pub status: String,
    #[ts(optional)]
    pub detail: Option<String>,
    pub created_at: i64,
    #[ts(optional)]
    pub finished_at: Option<i64>,
    /// Actions this run proposed (populated on the detail endpoint).
    #[serde(default)]
    pub proposed_actions: Vec<i64>,
}

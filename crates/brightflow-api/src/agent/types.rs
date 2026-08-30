//! Wire types for the agent-run endpoints.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Body for `POST /api/agent/runs`.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StartAgentRunRequest {
    /// "narrate_insights" | "triage_insights" | "propose_categories"
    /// | "propose_subcategories" | "propose_feedback_categories"
    pub kind: String,
    pub source_id: String,
    pub table: String,
    /// Parent category id for `propose_subcategories` (required there,
    /// ignored elsewhere).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub parent_id: Option<i64>,
    /// "auto_apply" (default) | "propose". Auto-apply is the default because
    /// every action the agent runner hands out is undoable — reversibility,
    /// not pre-approval, is the safety mechanism.
    #[serde(default)]
    #[ts(optional)]
    pub mode: Option<String>,
}

/// One agent run, as returned by the API.
#[derive(Debug, Serialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResponse {
    // `number`, not the default `bigint`: these arrive via JSON.parse as plain
    // numbers at runtime, and sqlite rowids / epoch-ms stay well inside 2^53.
    #[ts(type = "number")]
    pub id: i64,
    pub kind: String,
    pub mode: String,
    pub scope: String,
    pub status: String,
    #[ts(optional)]
    pub detail: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(optional, type = "number")]
    pub finished_at: Option<i64>,
    /// Actions this run proposed (populated on the detail endpoint).
    #[serde(default)]
    #[ts(type = "number[]")]
    pub proposed_actions: Vec<i64>,
}

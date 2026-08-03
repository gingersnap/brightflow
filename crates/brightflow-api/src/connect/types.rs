//! Wire types for the connector management endpoints.
//!
//! The `Unified*` shapes exist because the UI shows connectors, their schedules,
//! and their run history as one thing; assembling that server-side keeps the
//! frontend from making three calls and joining them itself.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Request body for POST /connectors/:name/schedule
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRequest {
    pub interval_secs: i64,
}

/// Response for POST /connectors/:name/schedule
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleResponse {
    pub job_id: String,
    pub connector_config_id: String,
    pub interval_secs: i64,
    pub enabled: bool,
}

/// Response for POST /connectors/:name/run
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RunTriggerResponse {
    pub run_id: String,
    pub connector: String,
    pub status: String,
}

/// Request body for PUT /connectors/:name/token
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateTokenRequest {
    pub token: String,
}

/// Unified connector view — config + schedule + latest run
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedConnector {
    pub name: String,
    pub connector: String,
    pub valid: bool,
    pub has_token: bool,
    pub job: Option<UnifiedJob>,
    pub last_run: Option<UnifiedSyncRun>,
}

/// Schedule info for a connector
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedJob {
    pub id: String,
    // `number`, not the default `bigint`: arrives via JSON.parse as a plain
    // number at runtime, and an interval in seconds stays well inside 2^53.
    #[ts(type = "number")]
    pub interval_secs: i64,
    pub enabled: bool,
}

/// Last run info for a connector
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedSyncRun {
    pub id: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub rows_synced: i64,
    pub error: Option<String>,
}

/// Discovered connector with its presets
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AvailableConnectorResponse {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub source_type: String,
    pub source_hash: String,
    pub presets: Vec<PresetInfo>,
}

/// Summary of a config preset
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
    pub has_token: bool,
}

/// Enriched sync run with connector name resolved
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedSyncRun {
    pub id: String,
    pub connector_name: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub rows_synced: i64,
    pub error: Option<String>,
}

use serde::{Deserialize, Serialize};

/// Info about a configured connector (from TOML config on disk)
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorInfo {
    /// Name derived from config filename (e.g., "github")
    pub name: String,
    /// Connector type from the config (same as name for built-ins)
    pub connector: String,
    /// Whether the matching .lua connector file exists
    pub valid: bool,
}

/// Optional request body for POST /connectors/:name/run
#[derive(Debug, Deserialize)]
pub struct RunRequest {
    /// Only sync specific endpoints (comma-separated)
    pub only: Option<String>,
}

/// Request body for POST /connectors/:name/schedule
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRequest {
    pub interval_secs: i64,
}

/// Response for POST /connectors/:name/schedule
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleResponse {
    pub job_id: String,
    pub connector_config_id: String,
    pub interval_secs: i64,
    pub enabled: bool,
}

/// Response for POST /connectors/:name/run
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTriggerResponse {
    pub run_id: String,
    pub connector: String,
    pub status: String,
}

/// Unified connector view — combines file config + DB schedule + latest run
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedConnector {
    pub name: String,
    pub connector: String,
    pub valid: bool,
    pub job: Option<UnifiedJob>,
    pub last_run: Option<UnifiedSyncRun>,
}

/// Schedule info for a connector
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedJob {
    pub id: String,
    pub interval_secs: i64,
    pub enabled: bool,
}

/// Last run info for a connector
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedSyncRun {
    pub id: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub rows_synced: i64,
    pub error: Option<String>,
}

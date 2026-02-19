use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Info about a configured connector (from YAML config on disk)
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorInfo {
    /// Name derived from config filename (e.g., "github")
    pub name: String,
    /// Connector type from the config (same as name for built-ins)
    pub connector: String,
    /// Whether the matching .lua connector file exists
    pub valid: bool,
}

/// Status of a connector run
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// A single connector run tracked in memory
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorRun {
    pub id: Uuid,
    pub connector: String,
    pub config_name: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub endpoints_synced: Vec<String>,
    pub tables_ingested: Vec<String>,
    pub error: Option<String>,
}

/// Response returned when a run is triggered
#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub run_id: Uuid,
    pub connector: String,
    pub status: RunStatus,
}

/// Optional request body for POST /connectors/:name/run
#[derive(Debug, Deserialize)]
pub struct RunRequest {
    /// Only sync specific endpoints (comma-separated)
    pub only: Option<String>,
}

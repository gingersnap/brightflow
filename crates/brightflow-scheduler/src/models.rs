//! Row types for the scheduler database, shared with the frontend via ts-rs.
//!
//! Timestamps are `String` rather than `DateTime` because SQLite stores them as
//! ISO-8601 text and every consumer (the API, the UI) wants that text — parsing
//! to a typed value here would only be re-serialized at the boundary.
//!
//! `SyncState` is the incremental-sync cursor, keyed by (connector, endpoint):
//! it is what makes a re-run fetch only new rows rather than everything.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// --- Connector Config ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorConfig {
    pub id: String,
    pub name: String,
    pub connector_path: String,
    pub config_json: String,
    pub token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// --- Scheduler Job ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerJob {
    pub id: String,
    pub name: String,
    pub connector_id: String,
    pub interval_secs: i64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

// --- Sync State ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub connector_id: String,
    pub endpoint: String,
    pub cursor_field: Option<String>,
    pub cursor_value: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_status: Option<String>,
    pub rows_synced: i64,
}

// --- Sync Run ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SyncRun {
    pub id: String,
    pub job_id: Option<String>,
    pub connector_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub endpoints_synced: Option<String>,
    pub rows_synced: i64,
    pub error: Option<String>,
}

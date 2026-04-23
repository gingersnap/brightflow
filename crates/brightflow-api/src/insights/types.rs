use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Request to run a review analysis
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRequest {
    pub source_id: String,
    pub dataset_id: String,
    /// Cadence: "daily", "weekly", or "monthly"
    #[serde(default = "default_cadence")]
    pub cadence: String,
}

fn default_cadence() -> String {
    "weekly".to_string()
}

/// Request to run a trends analysis
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TrendsRequest {
    pub source_id: String,
    pub dataset_id: String,
}

/// Response from an insights analysis
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InsightsResponse {
    pub dataset_id: String,
    pub report_type: String,
    #[ts(type = "unknown")]
    pub tree: serde_json::Value,
    pub node_count: usize,
    pub finding_count: usize,
    pub first_level_count: usize,
    pub deeper_count: usize,
    pub execution_time_ms: f64,
}

use serde::{Deserialize, Serialize};

/// Request to run a review analysis
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRequest {
    pub dataset_id: String,
    /// Cadence: "daily", "weekly", or "monthly"
    #[serde(default = "default_cadence")]
    pub cadence: String,
}

fn default_cadence() -> String {
    "weekly".to_string()
}

/// Request to run a trends analysis
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendsRequest {
    pub dataset_id: String,
}

/// Response from an insights analysis
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsResponse {
    pub dataset_id: String,
    pub report_type: String,
    pub tree: serde_json::Value,
    pub node_count: usize,
    pub finding_count: usize,
    pub execution_time_ms: f64,
}

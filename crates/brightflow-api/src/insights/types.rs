use brightflow_engine::analysis::tree::AnalysisTree;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Optional engine config knobs accepted on insights requests
#[derive(Debug, Serialize, Deserialize, TS, Default, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EngineConfig {
    #[ts(optional)]
    pub z_threshold: Option<f64>,
    #[ts(optional)]
    pub p_threshold: Option<f64>,
    #[ts(optional)]
    pub min_effect_size: Option<f64>,
    #[ts(optional)]
    pub max_results: Option<usize>,
    #[ts(optional)]
    pub max_depth: Option<usize>,
}

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
    #[serde(default)]
    pub config: EngineConfig,
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
    #[serde(default)]
    pub config: EngineConfig,
}

/// Response from an insights analysis
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InsightsResponse {
    pub dataset_id: String,
    pub report_type: String,
    pub tree: AnalysisTree,
    pub node_count: usize,
    pub finding_count: usize,
    pub first_level_count: usize,
    pub deeper_count: usize,
    pub execution_time_ms: f64,
    /// How many candidate analyses ran in total (visible to users as "X of Y")
    pub total_candidates: usize,
    /// Effective config used (after request overrides + defaults)
    pub config_used: EngineConfig,
}

//! Wire types for the insights endpoints.
//!
//! `EngineConfig` carries every knob as an `Option` so a request can override
//! one threshold without restating the defaults — and so adding a knob never
//! breaks an existing client.

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
    /// Diversity-selected top-N roots returned (formerly `maxResults`)
    #[ts(optional)]
    #[serde(alias = "maxResults")]
    pub select_top: Option<usize>,
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

/// Request to run a drivers analysis
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DriversRequest {
    pub source_id: String,
    pub dataset_id: String,
    #[serde(default)]
    pub config: EngineConfig,
}

/// One recorded insights computation (manual or post-sync).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InsightRunResponse {
    #[ts(type = "number")]
    pub id: i64,
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
    pub execution_time_ms: f64,
    /// Unix epoch seconds
    #[ts(type = "number")]
    pub computed_at: i64,
}

impl InsightRunResponse {
    pub fn from_row(row: brightflow_store::InsightRunRow) -> Self {
        Self {
            id: row.id,
            source_id: row.source_id,
            table: row.table_name,
            report_type: row.report_type,
            triggered_by: row.triggered_by,
            finding_count: row.finding_count,
            new_finding_count: row.new_finding_count,
            top_summary: row.top_summary,
            execution_time_ms: row.execution_time_ms,
            computed_at: row.computed_at,
        }
    }
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

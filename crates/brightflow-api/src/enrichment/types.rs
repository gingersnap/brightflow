//! Request/response types for enrichment-function endpoints.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Body for creating a draft function. `config` is the kind-specific spec
/// payload (the `kind` field is injected server-side from `kind`).
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateFunctionRequest {
    pub name: String,
    /// 'llm_prompt' | 'topic_model' | 'classifier'
    pub kind: String,
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
}

/// Body for editing a function's config (creates a new version).
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFunctionRequest {
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
    /// What to recompute after the edit: 'none' (default) | 'all' | 'missing'.
    #[serde(default)]
    #[ts(optional)]
    pub rerun: Option<String>,
}

/// One enrichment function with its current config.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EnrichFunctionResponse {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// 'draft' | 'promoted'
    pub status: String,
    #[ts(type = "number")]
    pub version: i64,
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
    /// Approximate rows not yet computed under the current spec
    /// (llm_prompt only).
    #[ts(type = "number | null")]
    pub stale_row_count: Option<i64>,
    /// Id of the currently running (non-sample) run, if any.
    #[ts(optional)]
    pub active_run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One immutable config version.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FunctionVersionResponse {
    #[ts(type = "number")]
    pub version: i64,
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
    pub created_at: String,
}

/// Body for a synchronous sample run over the first N rows.
#[derive(Debug, Default, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SampleRunRequest {
    /// Rows to sample (default 10, capped at 100).
    #[ts(optional)]
    pub limit: Option<usize>,
    /// Unsaved draft config to test instead of the stored version.
    #[serde(default)]
    #[ts(type = "unknown", optional)]
    pub config: Option<serde_json::Value>,
}

/// One sampled cell result.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SampleCellResponse {
    /// Content hash of the rendered inputs — stable row identity across runs.
    pub row_key: String,
    #[ts(type = "Record<string, string>")]
    pub inputs: BTreeMap<String, String>,
    /// Validated output object (one key per output field); None on error.
    #[ts(type = "unknown", optional)]
    pub value: Option<serde_json::Value>,
    /// 'ok' | 'error'
    pub status: String,
    #[ts(optional)]
    pub error: Option<String>,
    pub cached: bool,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SampleRunResponse {
    pub rows: Vec<SampleCellResponse>,
    #[ts(type = "number")]
    pub prompt_tokens: u64,
    #[ts(type = "number")]
    pub completion_tokens: u64,
    #[ts(type = "number")]
    pub total_tokens: u64,
    pub cache_hits: usize,
}

/// Pre-run token estimate. Approximate by design (p75 of history).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EstimateResponse {
    #[ts(type = "number")]
    pub rows_total: i64,
    #[ts(type = "number")]
    pub rows_uncached: i64,
    /// Rows the requested scope would actually run.
    #[ts(type = "number")]
    pub rows_to_run: i64,
    #[ts(type = "number | null")]
    pub p75_tokens_per_row: Option<i64>,
    #[ts(type = "number")]
    pub estimated_tokens: i64,
    /// 'history' (p75 of cached cells) | 'heuristic' (prompt length).
    pub basis: String,
}

/// Body for starting a full/incremental run.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StartEnrichRunRequest {
    /// 'missing' | 'all' | 'failed'
    pub scope: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StartEnrichRunResponse {
    pub run_id: String,
}

/// Run status (poll target).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EnrichRunResponse {
    pub id: String,
    pub function_id: String,
    #[ts(type = "number")]
    pub version: i64,
    /// 'sample' | 'full' | 'incremental'
    pub mode: String,
    /// 'running' | 'completed' | 'failed' | 'cancelled'
    pub status: String,
    #[ts(type = "number")]
    pub rows_total: i64,
    #[ts(type = "number")]
    pub rows_done: i64,
    #[ts(type = "number")]
    pub rows_failed: i64,
    #[ts(type = "number")]
    pub rows_cached: i64,
    #[ts(type = "number")]
    pub prompt_tokens: i64,
    #[ts(type = "number")]
    pub completion_tokens: i64,
    #[ts(type = "number")]
    pub total_tokens: i64,
    #[ts(optional)]
    pub error: Option<String>,
    pub created_at: String,
    #[ts(optional)]
    pub finished_at: Option<String>,
}

impl From<brightflow_store::EnrichmentRunRow> for EnrichRunResponse {
    fn from(row: brightflow_store::EnrichmentRunRow) -> Self {
        Self {
            id: row.id,
            function_id: row.function_id,
            version: row.version,
            mode: row.mode,
            status: row.status,
            rows_total: row.rows_total,
            rows_done: row.rows_done,
            rows_failed: row.rows_failed,
            rows_cached: row.rows_cached,
            prompt_tokens: row.prompt_tokens,
            completion_tokens: row.completion_tokens,
            total_tokens: row.total_tokens,
            error: row.error,
            created_at: row.created_at,
            finished_at: row.finished_at,
        }
    }
}

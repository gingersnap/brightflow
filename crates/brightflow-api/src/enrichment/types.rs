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
    /// 'ticket_classify' | 'ticket_extract'
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
    /// Prompt tokens the provider served from its prefix cache.
    #[ts(type = "number")]
    pub cached_tokens: u64,
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
    /// Prompt tokens the provider served from its prefix cache.
    #[ts(type = "number")]
    pub cached_tokens: i64,
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
            cached_tokens: row.cached_tokens,
            error: row.error,
            created_at: row.created_at,
            finished_at: row.finished_at,
        }
    }
}

/// Token totals for one source language.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UsageByLanguageRow {
    /// BCP-47 primary subtag, or `unknown`.
    pub language: String,
    /// Table rows in this language.
    pub rows: usize,
    /// Distinct cached cells (rows sharing content share a cell).
    pub cells: usize,
    #[ts(type = "number")]
    pub prompt_tokens: i64,
    #[ts(type = "number")]
    pub completion_tokens: i64,
    #[ts(type = "number")]
    pub cached_tokens: i64,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UsageByLanguageResponse {
    pub function_id: String,
    pub languages: Vec<UsageByLanguageRow>,
}

/// One vocabulary level's health (see the engine's `vocabulary::health`).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyLevelHealth {
    /// category | subcategory | feedback_category | product | competitor
    pub kind: String,
    pub entries: usize,
    pub cap: usize,
    pub rows: usize,
    /// Share of rows in `other`; above ~0.15 the vocabulary is wrong.
    pub other_rate: f64,
    pub max_share: f64,
    pub min_share: f64,
    /// Entries outside the 2 %–40 % balance band, with their share.
    pub unbalanced: Vec<UnbalancedEntry>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UnbalancedEntry {
    pub name: String,
    pub share: f64,
}

impl From<brightflow_engine::enrichment::LevelHealth> for VocabularyLevelHealth {
    fn from(h: brightflow_engine::enrichment::LevelHealth) -> Self {
        Self {
            kind: h.kind.as_str().to_string(),
            entries: h.entries,
            cap: h.cap,
            rows: h.rows,
            other_rate: h.other_rate,
            max_share: h.max_share,
            min_share: h.min_share,
            unbalanced: h
                .unbalanced
                .into_iter()
                .map(|(name, share)| UnbalancedEntry { name, share })
                .collect(),
        }
    }
}

/// Subcategory health under one parent category.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyParentHealth {
    pub parent: String,
    pub health: VocabularyLevelHealth,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyHealthResponse {
    /// Rows with a non-null `category`.
    pub classified_rows: usize,
    pub total_rows: usize,
    /// The root level (categories), when the table has been classified.
    pub levels: Vec<VocabularyLevelHealth>,
    pub per_parent: Vec<VocabularyParentHealth>,
    /// Fewest rows a level (or a parent's rows, for subcategories) must have
    /// before an induction run will propose entries for it. Stated here so
    /// the UI can disable "propose" with the reason rather than start a run
    /// that fails.
    pub induction_min_rows: usize,
}

/// Headline numbers for one mentioned subject.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MentionSubjectStats {
    /// product | competitor | pricing | service | feedback
    pub mention_type: String,
    /// Resolved entry name, or the unresolved surface.
    pub subject: String,
    pub resolved: bool,
    #[ts(type = "number")]
    pub mentions: i64,
    /// The headline: repeated complaints and long calls inflate `mentions`.
    #[ts(type = "number")]
    pub distinct_tickets: i64,
    /// Shares of this subject's mentions by sentiment. `mixed` and `neutral`
    /// count toward neither, so the two do not sum to one.
    pub negative_share: f64,
    pub positive_share: f64,
    pub incidental_share: f64,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct MentionSummaryResponse {
    pub mentions_table: String,
    pub total_mentions: usize,
    pub subjects: Vec<MentionSubjectStats>,
}

/// One unresolved-subject queue entry.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedSubjectResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub kind: String,
    pub surface: String,
    #[ts(type = "number")]
    pub mention_count: i64,
    #[ts(type = "number")]
    pub first_seen: i64,
    #[ts(type = "number")]
    pub last_seen: i64,
    /// open | mapped | ignored
    pub status: String,
    #[ts(optional, type = "number")]
    pub mapped_to: Option<i64>,
}

impl From<brightflow_store::UnresolvedSubjectRow> for UnresolvedSubjectResponse {
    fn from(r: brightflow_store::UnresolvedSubjectRow) -> Self {
        Self {
            id: r.id,
            kind: r.kind,
            surface: r.surface,
            mention_count: r.mention_count,
            first_seen: r.first_seen,
            last_seen: r.last_seen,
            status: r.status,
            mapped_to: r.mapped_to,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUnresolvedRequest {
    /// open | mapped | ignored
    pub status: String,
    /// Vocabulary entry to alias the surface onto (required for `mapped`).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub mapped_to: Option<i64>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ImportVocabularyResponse {
    pub applied: usize,
    pub total: usize,
    /// 1-based line (header is line 1) where the import stopped.
    #[ts(optional)]
    pub failed_line: Option<usize>,
    #[ts(optional)]
    pub error: Option<String>,
}

/// One vocabulary entry as the UI sees it.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyCategory {
    /// `number`, not ts-rs's default `bigint` for i64: the wire value is a
    /// plain JSON number, and a real BigInt would break JSON.stringify on
    /// the round trip.
    #[ts(type = "number")]
    pub id: i64,
    /// category | subcategory | feedback_category | product | competitor
    pub kind: String,
    /// 0 = root.
    #[ts(type = "number")]
    pub parent_id: i64,
    pub name: String,
    #[ts(optional)]
    pub description: Option<String>,
    pub frozen: bool,
}

/// A table's vocabularies, every kind and level in one flat list.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyOverview {
    pub categories: Vec<TaxonomyCategory>,
}

/// One value of a categorical column with its row count.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ValueCount {
    pub value: String,
    pub rows: usize,
}

/// One category with its subcategory breakdown.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CategoryCount {
    pub category: String,
    pub rows: usize,
    pub subcategories: Vec<ValueCount>,
}

/// Ticket-grain counts from the classifier's materialised columns.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TicketSummaryResponse {
    pub total_rows: usize,
    /// Rows with a non-null `category`.
    pub classified_rows: usize,
    pub categories: Vec<CategoryCount>,
    pub sentiment: Vec<ValueCount>,
    pub languages: Vec<ValueCount>,
}

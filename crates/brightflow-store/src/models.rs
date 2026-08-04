//! One Rust struct per SQLite table, deliberately mirroring column names 1:1.
//!
//! These are sqlx row mappings, not domain types: they stay stringly-typed
//! (status/role/polarity as TEXT) so the schema in migrations is the single
//! source of truth and a migration cannot silently disagree with an enum here.
//! Parsing into richer types happens at the call sites that need it.

use serde::{Deserialize, Serialize};

/// A table row from the `tables` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TableRow {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub schema_json: Option<String>,
    pub primary_keys: Option<String>,
    pub total_rows: i64,
    pub created_at: String,
    pub updated_at: String,
    pub partition_columns: Option<String>,
    pub source_id: String,
}

/// A file entry row from the `table_files` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TableFileRow {
    pub id: String,
    pub table_id: String,
    pub path: String,
    pub num_rows: i64,
    pub size_bytes: i64,
    pub added_at: String,
}

/// Column-level statistics from the `table_column_stats` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ColumnStatRow {
    pub table_id: String,
    pub column_name: String,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub null_count: Option<i64>,
}

/// Per-file column statistics for file-level pruning
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FileColumnStatRow {
    pub file_id: String,
    pub column_name: String,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub null_count: Option<i64>,
}

/// Column-level semantic override (user-defined role for insights analysis)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ColumnSemanticRow {
    pub table_id: String,
    pub column_name: String,
    pub role: String,
    pub is_kpi: bool,
    /// 'higher_is_better' | 'lower_is_better' | 'neutral'
    pub polarity: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub updated_at: String,
}

/// Table-level analysis settings override
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TableAnalysisSettingsRow {
    pub table_id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub time_granularity: Option<String>,
    pub comparison_periods: Option<i32>,
    pub updated_at: String,
}

/// Table-level text-enrichment settings override
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TableEnrichmentSettingsRow {
    pub table_id: String,
    /// JSON array of text column names to embed
    pub text_columns: Option<String>,
    pub cleaning_profile: Option<String>,
    pub language_column: Option<String>,
    pub embedder: Option<String>,
    pub min_cluster_size: Option<i64>,
    /// 'kmeans' | 'hdbscan'
    pub algorithm: Option<String>,
    pub updated_at: String,
}

/// One shown-insight history record (novelty decay input)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InsightHistoryRow {
    pub table_id: String,
    pub fingerprint: String,
    pub identity: String,
    pub insight_type: String,
    /// Direction + magnitude decile of the last shown value
    pub last_value_sig: String,
    pub shown_count: i64,
    /// Unix epoch seconds
    pub first_shown_at: i64,
    pub last_shown_at: i64,
}

/// User curation state for one insight (dismissed / pinned)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InsightStateRow {
    pub table_id: String,
    pub fingerprint: String,
    /// 'dismissed' | 'pinned'
    pub state: String,
    pub reason: Option<String>,
    pub annotation: Option<String>,
    pub created_at: i64,
}

/// Broad suppression: never surface insights about a segment or column
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InsightSuppressionRow {
    pub id: i64,
    pub table_id: String,
    /// 'segment' | 'column'
    pub kind: String,
    pub target: String,
    pub created_at: i64,
}

/// One insights computation (manual or post-sync) — badge + history input.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InsightRunRow {
    pub id: i64,
    pub table_id: String,
    pub source_id: String,
    pub table_name: String,
    pub report_type: String,
    /// 'manual' | 'post_sync'
    pub triggered_by: String,
    pub finding_count: i64,
    /// Roots whose fingerprints had never been shown before this run
    pub new_finding_count: i64,
    pub top_summary: Option<String>,
    pub execution_time_ms: f64,
    /// Unix epoch seconds
    pub computed_at: i64,
}

/// One entry in the first-class action log.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ActionLogRow {
    pub id: i64,
    pub request_id: String,
    /// 'human' | 'agent'
    pub actor_type: String,
    pub agent_run_id: Option<i64>,
    pub action_kind: String,
    pub params_json: String,
    pub result_json: Option<String>,
    /// Serialized inverse action; None = not undoable
    pub undo_json: Option<String>,
    /// 'applied' | 'proposed' | 'rejected' | 'undone' | 'failed'
    pub status: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

/// One background agent-run record.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentRunRow {
    pub id: i64,
    pub kind: String,
    pub mode: String,
    pub scope: String,
    pub status: String,
    pub detail: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

/// One durable cluster edit (curation overlay).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ClusterEditRow {
    pub id: i64,
    pub table_id: String,
    pub centroid_fingerprint: String,
    /// JSON array of f32 — centroid snapshot for reconciliation
    pub centroid_json: String,
    pub cluster_id: Option<i64>,
    pub custom_name: Option<String>,
    pub label: Option<String>,
    pub is_noise: bool,
    pub merged_into: Option<i64>,
    pub orphaned: bool,
    pub updated_at: i64,
}

/// One excluded naming term.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExcludedTermRow {
    pub id: i64,
    pub table_id: String,
    pub term: String,
    pub created_at: i64,
}

/// One intent category — an entry in the supervised taxonomy vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaxonomyCategoryRow {
    pub id: i64,
    pub table_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: i64,
}

/// One ROW-level intent label.
///
/// Row-level (not cluster-level) on purpose: cluster labels would re-teach the
/// format bias the classifier exists to defeat.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentLabelRow {
    pub id: i64,
    pub table_id: String,
    pub row_id: String,
    pub category_id: i64,
    /// "agent" (proposed) or "human" (ratified). Human wins on conflict.
    pub source: String,
    pub created_at: i64,
}

/// A document label joined to its category name — what training and the
/// curation UI actually need.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentLabelWithName {
    pub row_id: String,
    pub category_id: i64,
    pub name: String,
    pub source: String,
}

/// One registered connector-less source.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SourceRow {
    pub source_id: String,
    /// CHECK-constrained; the allowed set lives in the `sources` migration.
    pub kind: String,
    pub name: String,
    pub meta_json: Option<String>,
    pub created_at: String,
}

/// One enrichment function header (the versioned config lives in
/// `enrichment_function_versions`).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnrichmentFunctionRow {
    pub id: String,
    pub table_id: String,
    pub name: String,
    /// 'llm_prompt' | 'topic_model' | 'classifier'
    pub kind: String,
    /// 'draft' | 'promoted'
    pub status: String,
    pub current_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One immutable config snapshot of an enrichment function.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnrichmentFunctionVersionRow {
    pub function_id: String,
    pub version: i64,
    /// Serde form of the engine `FunctionSpec` (tagged on "kind").
    pub config_json: String,
    pub created_at: String,
}

/// One enrichment run (sample runs are not recorded here; only full and
/// incremental runs).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnrichmentRunRow {
    pub id: String,
    pub function_id: String,
    pub version: i64,
    /// 'sample' | 'full' | 'incremental'
    pub mode: String,
    /// 'running' | 'completed' | 'failed' | 'cancelled'
    pub status: String,
    pub rows_total: i64,
    pub rows_done: i64,
    pub rows_failed: i64,
    pub rows_cached: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

/// One cached per-cell enrichment result. Errors are cached too so a full
/// re-run doesn't hammer the provider with known-bad rows; `scope=failed`
/// clears them first.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnrichmentCacheRow {
    pub function_id: String,
    pub spec_hash: String,
    pub input_hash: String,
    /// 'ok' | 'error'
    pub status: String,
    pub value_json: Option<String>,
    pub error: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    /// Bookkeeping only — never part of the cache key.
    pub version: i64,
    pub created_at: String,
}

//! SQLite row types for Litehouse metadata

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

/// A partition key/value pair for a file
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FilePartitionRow {
    pub file_id: String,
    pub partition_key: String,
    pub partition_value: String,
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

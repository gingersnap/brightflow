//! One Rust struct per SQLite table, deliberately mirroring column names 1:1.
//!
//! These are rusqlite row mappings, not domain types: they stay stringly-typed
//! (status/role/polarity as TEXT) so the schema in migrations is the single
//! source of truth and a migration cannot silently disagree with an enum here.
//! The semantic rows convert to and from the contract crate's types in
//! `db::semantics`, the one place a stored string becomes an enum; everything
//! else parses at the call site that needs it. The `impl_from_row!` blocks at
//! the bottom are what make the 1:1 mirroring load-bearing: each field is
//! read from the column of the same name.

use serde::{Deserialize, Serialize};

/// A table row from the `tables` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableFileRow {
    pub id: String,
    pub table_id: String,
    pub path: String,
    pub num_rows: i64,
    pub size_bytes: i64,
    pub added_at: String,
}

/// Column-level statistics from the `table_column_stats` SQLite table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStatRow {
    pub table_id: String,
    pub column_name: String,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub null_count: Option<i64>,
}

/// Per-file column statistics for file-level pruning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileColumnStatRow {
    pub file_id: String,
    pub column_name: String,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub null_count: Option<i64>,
}

/// One layer's opinion about one column: a row of `column_semantics`, keyed
/// by (table, column, layer, producer). Every semantic field is nullable —
/// NULL is "no opinion at this layer".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSemanticRow {
    pub table_id: String,
    pub column_name: String,
    /// 'detected' | 'declared' | 'agent' | 'user'
    pub layer: String,
    pub producer: String,
    pub producer_version: Option<String>,
    pub producer_hash: Option<String>,
    /// One of the ten logical types, as spelled by `LogicalType::as_str`.
    pub datatype: Option<String>,
    pub is_time: Option<bool>,
    /// 'measure' | 'dimension' | 'time' | 'entity' | 'ignored'
    pub role: Option<String>,
    pub is_kpi: Option<bool>,
    /// 'higher_is_better' | 'lower_is_better' | 'neutral'
    pub polarity: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub ai_context_json: Option<String>,
    pub extensions_json: Option<String>,
    pub updated_at: String,
}

/// One layer's opinion about a table: a row of `table_semantics`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSemanticsRow {
    pub table_id: String,
    pub layer: String,
    pub producer: String,
    pub producer_version: Option<String>,
    pub producer_hash: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub time_granularity: Option<String>,
    pub comparison_periods: Option<i64>,
    pub doc_json: Option<String>,
    pub ai_context_json: Option<String>,
    pub extensions_json: Option<String>,
    pub updated_at: String,
}

/// One many-to-one relationship between two tables of a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipRow {
    pub id: i64,
    pub source_id: String,
    pub name: String,
    pub from_table_id: String,
    pub to_table_id: String,
    /// JSON array of column names.
    pub from_columns_json: String,
    pub to_columns_json: String,
    pub layer: String,
    pub producer: String,
    pub producer_version: Option<String>,
    pub ai_context_json: Option<String>,
    pub extensions_json: Option<String>,
    pub updated_at: String,
}

/// One named metric over one table. `expr_json` is the structured
/// `MetricExpr`; `sql` is its rendered ANSI form, kept for export only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRow {
    pub id: i64,
    pub table_id: String,
    pub name: String,
    pub expr_json: String,
    pub sql: String,
    pub datatype: Option<String>,
    pub description: Option<String>,
    pub is_kpi: Option<bool>,
    pub polarity: Option<String>,
    pub format: Option<String>,
    pub layer: String,
    pub producer: String,
    pub producer_version: Option<String>,
    pub ai_context_json: Option<String>,
    pub extensions_json: Option<String>,
    pub updated_at: String,
}

/// One shown-insight history record (novelty decay input)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightSuppressionRow {
    pub id: i64,
    pub table_id: String,
    /// 'segment' | 'column'
    pub kind: String,
    pub target: String,
    pub created_at: i64,
}

/// One insights computation (manual or post-sync) — badge + history input.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLogRow {
    pub id: i64,
    pub request_id: String,
    /// 'human' | 'agent'
    pub actor_type: String,
    pub agent_run_id: Option<i64>,
    /// The human who acted; None for agent rows and pre-audit rows.
    pub user_id: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// One vocabulary entry.
///
/// An induced category / subcategory / feedback_category, or an imported
/// product / competitor. `parent_id` is 0 for roots; `frozen` rows refuse
/// rename and redefine; `aliases_json` is a JSON array of accepted surface
/// forms (imported kinds only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyCategoryRow {
    pub id: i64,
    pub table_id: String,
    pub kind: String,
    pub parent_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub frozen: bool,
    pub aliases_json: Option<String>,
    pub created_at: i64,
}

/// One unresolved subject surface from mention extraction (review queue).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedSubjectRow {
    pub id: i64,
    pub table_id: String,
    /// product | competitor | pricing | service
    pub kind: String,
    /// The model's normalised name for the entity.
    pub surface: String,
    pub mention_count: i64,
    pub first_seen: i64,
    pub last_seen: i64,
    /// open | mapped | ignored
    pub status: String,
    pub mapped_to: Option<i64>,
}

/// One registered source: an upload, a connector, or a web site. The
/// producer columns say which connector at which version made its tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRow {
    pub source_id: String,
    /// CHECK-constrained; the allowed set lives in the `sources` migration.
    pub kind: String,
    pub name: String,
    pub meta_json: Option<String>,
    pub producer: Option<String>,
    pub producer_version: Option<String>,
    pub producer_hash: Option<String>,
    pub created_at: String,
}

/// One enrichment function header (the versioned config lives in
/// `enrichment_function_versions`).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentFunctionVersionRow {
    pub function_id: String,
    pub version: i64,
    /// Serde form of the engine `FunctionSpec` (tagged on "kind").
    pub config_json: String,
    pub created_at: String,
}

/// One enrichment run (sample runs are not recorded here; only full and
/// incremental runs).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Prompt tokens the provider served from its prefix cache.
    pub cached_tokens: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

/// One cached per-cell enrichment result. Errors are cached too so a full
/// re-run doesn't hammer the provider with known-bad rows; `scope=failed`
/// clears them first.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// None = not reported by the provider.
    pub cached_tokens: Option<i64>,
    /// Bookkeeping only — never part of the cache key.
    pub version: i64,
    pub created_at: String,
}

// Row mappings, one per struct above, fields matching columns by name.
crate::impl_from_row!(TableRow {
    id,
    name,
    version,
    schema_json,
    primary_keys,
    total_rows,
    created_at,
    updated_at,
    partition_columns,
    source_id,
});
crate::impl_from_row!(TableFileRow {
    id,
    table_id,
    path,
    num_rows,
    size_bytes,
    added_at
});
crate::impl_from_row!(ColumnStatRow {
    table_id,
    column_name,
    min_value,
    max_value,
    null_count
});
crate::impl_from_row!(FileColumnStatRow {
    file_id,
    column_name,
    min_value,
    max_value,
    null_count
});
crate::impl_from_row!(ColumnSemanticRow {
    table_id,
    column_name,
    layer,
    producer,
    producer_version,
    producer_hash,
    datatype,
    is_time,
    role,
    is_kpi,
    polarity,
    label,
    description,
    ai_context_json,
    extensions_json,
    updated_at,
});
crate::impl_from_row!(TableSemanticsRow {
    table_id,
    layer,
    producer,
    producer_version,
    producer_hash,
    display_name,
    description,
    time_granularity,
    comparison_periods,
    doc_json,
    ai_context_json,
    extensions_json,
    updated_at,
});
crate::impl_from_row!(RelationshipRow {
    id,
    source_id,
    name,
    from_table_id,
    to_table_id,
    from_columns_json,
    to_columns_json,
    layer,
    producer,
    producer_version,
    ai_context_json,
    extensions_json,
    updated_at,
});
crate::impl_from_row!(MetricRow {
    id,
    table_id,
    name,
    expr_json,
    sql,
    datatype,
    description,
    is_kpi,
    polarity,
    format,
    layer,
    producer,
    producer_version,
    ai_context_json,
    extensions_json,
    updated_at,
});
crate::impl_from_row!(InsightHistoryRow {
    table_id,
    fingerprint,
    identity,
    insight_type,
    last_value_sig,
    shown_count,
    first_shown_at,
    last_shown_at,
});
crate::impl_from_row!(InsightStateRow {
    table_id,
    fingerprint,
    state,
    reason,
    annotation,
    created_at
});
crate::impl_from_row!(InsightSuppressionRow {
    id,
    table_id,
    kind,
    target,
    created_at
});
crate::impl_from_row!(InsightRunRow {
    id,
    table_id,
    source_id,
    table_name,
    report_type,
    triggered_by,
    finding_count,
    new_finding_count,
    top_summary,
    execution_time_ms,
    computed_at,
});
crate::impl_from_row!(ActionLogRow {
    id,
    request_id,
    actor_type,
    agent_run_id,
    user_id,
    action_kind,
    params_json,
    result_json,
    undo_json,
    status,
    created_at,
    resolved_at,
});
crate::impl_from_row!(AgentRunRow {
    id,
    kind,
    mode,
    scope,
    status,
    detail,
    created_at,
    finished_at
});
crate::impl_from_row!(TaxonomyCategoryRow {
    id,
    table_id,
    kind,
    parent_id,
    name,
    description,
    frozen,
    aliases_json,
    created_at
});
crate::impl_from_row!(UnresolvedSubjectRow {
    id,
    table_id,
    kind,
    surface,
    mention_count,
    first_seen,
    last_seen,
    status,
    mapped_to
});
crate::impl_from_row!(SourceRow {
    source_id,
    kind,
    name,
    meta_json,
    producer,
    producer_version,
    producer_hash,
    created_at
});
crate::impl_from_row!(EnrichmentFunctionRow {
    id,
    table_id,
    name,
    kind,
    status,
    current_version,
    created_at,
    updated_at,
});
crate::impl_from_row!(EnrichmentFunctionVersionRow {
    function_id,
    version,
    config_json,
    created_at
});
crate::impl_from_row!(EnrichmentRunRow {
    id,
    function_id,
    version,
    mode,
    status,
    rows_total,
    rows_done,
    rows_failed,
    rows_cached,
    prompt_tokens,
    completion_tokens,
    cached_tokens,
    total_tokens,
    error,
    created_at,
    finished_at,
});
crate::impl_from_row!(EnrichmentCacheRow {
    function_id,
    spec_hash,
    input_hash,
    status,
    value_json,
    error,
    prompt_tokens,
    completion_tokens,
    cached_tokens,
    version,
    created_at,
});

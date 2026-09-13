//! Wire types for the unified source list, including which tools each source
//! kind exposes.

use serde::Serialize;
use ts_rs::TS;

/// The kind of data source
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    WebAnalytics,
    Connector,
    Upload,
}

/// Response for a persistent CSV upload (`POST /api/sources/upload`).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UploadSourceResponse {
    pub source_id: String,
    pub table: String,
    pub row_count: usize,
    pub columns: Vec<String>,
}

/// Tools available for a given source
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum SourceTool {
    Dashboard,
    Funnels,
    Retention,
    Users,
    Explore,
    Insights,
    Textexplore,
    /// Summary, classification and extraction over an LLM for any text
    /// table: setup, results and approvals in one tool.
    Textenrichment,
}

/// A table belonging to a source
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SourceTable {
    pub name: String,
    /// The resolved display name, when someone set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    /// The resolved description, when someone wrote one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(type = "number | null")]
    pub num_rows: Option<i64>,
    /// True when the table type supports text enrichment (Topics).
    pub enrichable: bool,
    /// The most recent re-declaration by a producer, when one changed
    /// something, so a card can say what a connector upgrade did.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_declaration_change: Option<brightflow_types::DeclarationDiff>,
    /// Present when the table is a model's output: which model, built from
    /// what.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<ModelBadge>,
}

/// The one line a table list says about a model.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ModelBadge {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub input_table: Option<String>,
}

/// Unified view of a data source (event source or connector)
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedSource {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub connector_name: Option<String>,
    pub domain: Option<String>,
    pub tables: Vec<SourceTable>,
    pub tools: Vec<SourceTool>,
    pub created_at: String,
    pub ready: bool,
}

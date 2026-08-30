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
    /// Ticket classification + mention extraction over an LLM: setup,
    /// results and approvals in one tool.
    Textanalytics,
}

/// A table belonging to a source
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SourceTable {
    pub name: String,
    #[ts(type = "number | null")]
    pub num_rows: Option<i64>,
    /// True when the table type supports text enrichment (Topics).
    pub enrichable: bool,
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

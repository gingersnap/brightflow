use serde::Serialize;
use ts_rs::TS;

/// The kind of data source
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    WebAnalytics,
    Connector,
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
}

/// A table belonging to a source
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SourceTable {
    pub name: String,
    #[ts(type = "number | null")]
    pub num_rows: Option<i64>,
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

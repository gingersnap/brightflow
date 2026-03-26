use crate::analytics::session::ColumnInfo;
use serde::{Deserialize, Serialize};

/// Main query structure - a chain of operations applied sequentially
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    /// ID of the dataset to query (defaults to "default")
    #[serde(default = "default_dataset_id")]
    pub dataset_id: String,

    /// Chain of operations to apply
    pub operations: Vec<Operation>,
}

fn default_dataset_id() -> String {
    "default".to_string()
}

/// Each operation transforms the DataFrame
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Operation {
    /// Filter rows by column condition
    Filter {
        column: String,
        op: FilterOp,
        #[serde(default)]
        value: serde_json::Value,
    },

    /// Select specific columns
    Select { columns: Vec<String> },

    /// Group by columns with aggregations
    GroupBy { by: Vec<String>, aggs: Vec<AggSpec> },

    /// Pivot table transformation
    Pivot {
        index: Vec<String>,
        columns: String,
        values: String,
        #[serde(default)]
        agg: Option<Aggregation>,
    },

    /// Sort by column(s)
    Sort {
        by: String,
        #[serde(default)]
        descending: bool,
    },

    /// Limit number of rows returned
    Limit { n: u32 },
}

/// Filter comparison operators
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    In,
    IsNull,
    IsNotNull,
}

/// Aggregation specification for GroupBy
#[derive(Debug, Clone, Deserialize)]
pub struct AggSpec {
    /// Column to aggregate ("*" for count)
    pub column: String,
    /// Aggregation function
    pub function: Aggregation,
    /// Optional output column name
    #[serde(default)]
    pub alias: Option<String>,
}

/// Aggregation functions
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Aggregation {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Median,
    Std,
    First,
    Last,
}

/// Query execution response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub total_rows: usize,
    pub execution_time_ms: f64,
}

// ============================================================================
// WebSocket Protocol
// ============================================================================

/// WebSocket message from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsClientMessage {
    /// Execute a query
    Query(Query),

    /// Heartbeat ping
    Ping,
}

/// WebSocket message to client
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsServerMessage {
    /// Query results
    QueryResult(QueryResponse),

    /// Error response
    Error { code: String, message: String },

    /// Heartbeat response
    Pong,

    /// Connection established acknowledgment
    Connected { server_version: String },
}

// ============================================================================
// REST API DTOs
// ============================================================================

/// Response for dataset upload
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    pub id: String,
    pub name: String,
    pub row_count: usize,
    pub column_count: usize,
    pub columns: Vec<ColumnInfo>,
}

/// Response for dataset metadata
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetMetadataResponse {
    pub id: String,
    pub name: String,
    pub row_count: Option<usize>,
    pub column_count: Option<usize>,
    pub columns: Vec<ColumnInfo>,
}

/// Response for loading a Delta table on-demand
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTableResponse {
    pub id: String,
    pub name: String,
    pub row_count: Option<usize>,
    pub column_count: Option<usize>,
    pub columns: Vec<ColumnInfo>,
}

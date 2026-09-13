//! Wire types for the query API: a `Query` (an operation chain over a
//! loaded dataset) and its response.
//!
//! The chain itself is the contract crate's operations language
//! (`brightflow_types::ops`), shared with model recipes and the action
//! manifest; this module adds only the session-bound wrapper and the
//! response and WebSocket frames.

use crate::analytics::session::ColumnInfo;
// One spelling of the chain, the aggregation functions and the filter
// operators on every wire.
use brightflow_types::TimeGranularity;
pub use brightflow_types::{AggSpec, Aggregation, DerivedColumn, DerivedExpr, FilterOp, Operation};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Main query structure - a chain of operations applied sequentially
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    /// ID of the dataset to query (defaults to "default")
    #[serde(default = "default_dataset_id")]
    pub dataset_id: String,

    /// Chain of operations to apply
    pub operations: Vec<Operation>,

    /// Client-chosen correlation id, echoed back on the response so a client
    /// with two in-flight queries can match answers to questions.
    #[serde(default)]
    #[ts(optional)]
    pub request_id: Option<String>,
}

fn default_dataset_id() -> String {
    "default".to_string()
}

/// Query execution response
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse {
    pub columns: Vec<ColumnInfo>,
    #[ts(type = "unknown[][]")]
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub total_rows: usize,
    pub execution_time_ms: f64,
    /// Echo of the query's correlation id; absent on the REST path.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub request_id: Option<String>,
}

// ============================================================================
// WebSocket Protocol
// ============================================================================

/// WebSocket message from client
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsClientMessage {
    /// Execute a query
    Query(Query),

    /// Heartbeat ping
    Ping,
}

/// WebSocket message to client
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsServerMessage {
    /// Query results
    QueryResult(QueryResponse),

    /// Error response
    Error {
        code: String,
        message: String,
        /// Correlation id when the failing query carried one; parse errors
        /// never do — the client accepts uncorrelated errors.
        #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        request_id: Option<String>,
    },

    /// Heartbeat response
    Pong,

    /// Connection established acknowledgment
    Connected {
        #[serde(rename = "serverVersion")]
        server_version: String,
    },

    /// One action-log row changed (created/applied/failed/rejected/undone)
    ActionEvent(crate::actions::events::ActionEventPayload),

    /// Many action-log rows changed at once (approve-all, undo-all)
    ActionBatch(crate::actions::events::ActionBatchPayload),

    /// An agent run started or changed status
    AgentRun(crate::actions::events::AgentRunEventPayload),

    /// The client missed events (lagged subscriber) — refetch feed and count
    ActionResync,

    /// An insights run (manual or post-sync) finished
    InsightsComputed(crate::actions::events::InsightsComputedPayload),

    /// A sync or enrichment run started, progressed or finished
    Job(crate::actions::events::JobEventPayload),
}

impl From<crate::actions::events::CurationEvent> for WsServerMessage {
    fn from(event: crate::actions::events::CurationEvent) -> Self {
        use crate::actions::events::CurationEvent;
        match event {
            CurationEvent::Action(p) => Self::ActionEvent(p),
            CurationEvent::ActionBatch(p) => Self::ActionBatch(p),
            CurationEvent::AgentRun(p) => Self::AgentRun(p),
            CurationEvent::InsightsComputed(p) => Self::InsightsComputed(p),
            CurationEvent::Job(p) => Self::Job(p),
        }
    }
}

// ============================================================================
// REST API DTOs
// ============================================================================

/// Response for dataset upload
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    pub id: String,
    pub name: String,
    pub row_count: usize,
    pub column_count: usize,
    pub columns: Vec<ColumnInfo>,
}

/// Response for dataset metadata
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DatasetMetadataResponse {
    pub id: String,
    pub name: String,
    pub row_count: Option<usize>,
    pub column_count: Option<usize>,
    pub columns: Vec<ColumnInfo>,
}

/// Response for loading a Parquet table on-demand
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LoadTableResponse {
    pub id: String,
    pub name: String,
    pub row_count: Option<usize>,
    pub column_count: Option<usize>,
    pub columns: Vec<ColumnInfo>,
    /// The table's configured period for time bucketing, when one is set.
    /// Explore uses it as the default granularity of a time field.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub time_granularity: Option<TimeGranularity>,
    /// The resolved display name, when someone set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    /// The resolved description, when someone wrote one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_request_id_defaults_to_none_and_parses_when_sent() {
        let bare: Query = serde_json::from_str(r#"{"operations":[]}"#).expect("parse");
        assert_eq!(bare.request_id, None);
        let tagged: Query =
            serde_json::from_str(r#"{"operations":[],"requestId":"r-1"}"#).expect("parse");
        assert_eq!(tagged.request_id.as_deref(), Some("r-1"));
    }

    #[test]
    fn request_id_serializes_only_when_present() {
        let mut resp = QueryResponse {
            columns: vec![],
            rows: vec![],
            row_count: 0,
            total_rows: 0,
            execution_time_ms: 1.0,
            request_id: None,
        };
        let bare = serde_json::to_string(&resp).expect("serialize");
        assert!(!bare.contains("requestId"), "absent id must not serialize");

        resp.request_id = Some("r-1".to_string());
        let tagged = serde_json::to_string(&WsServerMessage::QueryResult(resp)).expect("serialize");
        assert!(tagged.contains(r#""requestId":"r-1""#));

        let err = WsServerMessage::Error {
            code: "X".to_string(),
            message: "m".to_string(),
            request_id: None,
        };
        let err_json = serde_json::to_string(&err).expect("serialize");
        assert!(!err_json.contains("requestId"));
    }
}

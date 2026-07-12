use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One configured LLM provider (api_key redacted).
#[derive(Debug, Serialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderResponse {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    /// True when an api key is stored (the key itself is never returned).
    pub has_api_key: bool,
    pub model: String,
    pub is_default: bool,
}

/// Body for creating/updating a provider.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpsertLlmProviderRequest {
    pub name: String,
    pub base_url: String,
    /// Omit to keep the stored key; empty string clears it.
    #[ts(optional)]
    pub api_key: Option<String>,
    pub model: String,
    #[serde(default)]
    pub is_default: bool,
}

/// Result of a connection test.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestResponse {
    pub ok: bool,
    #[ts(optional)]
    pub error: Option<String>,
}

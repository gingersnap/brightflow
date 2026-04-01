use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A registered website/domain that sends analytics events.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub domain: String,
    pub name: String,
    pub timezone: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Incoming event from the tracking script (before enrichment).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEvent {
    /// Event name: "pageview" or a custom event name.
    pub name: String,
    /// Full page URL.
    pub url: String,
    /// Document referrer (may be empty).
    #[serde(default)]
    pub referrer: String,
    /// Screen width in pixels (for device classification).
    #[serde(default)]
    pub screen_width: Option<u16>,
    /// Source domain (used to look up source config).
    pub domain: String,
    /// Custom properties (key-value pairs, optional).
    #[serde(default)]
    pub props: Option<serde_json::Value>,
}

/// Enriched event (after processing). All fields are strings for flat Parquet columns.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Event {
    // Envelope
    pub id: String,
    pub timestamp: String,
    pub source_id: String,
    pub event_name: String,

    // Identity
    pub visitor_id: String,
    pub session_id: String,

    // Page
    pub hostname: String,
    pub pathname: String,
    pub page_url: String,

    // Referrer
    pub referrer: String,
    pub referrer_source: String,

    // UTM
    pub utm_source: String,
    pub utm_medium: String,
    pub utm_campaign: String,
    pub utm_content: String,
    pub utm_term: String,

    // Device (from User-Agent)
    pub browser: String,
    pub browser_version: String,
    pub os: String,
    pub os_version: String,
    pub device_type: String,
    pub screen_size: String,

    // Geo (from IP, IP discarded after)
    pub country: String,
    pub region: String,
    pub city: String,

    // Custom
    pub properties: String,
}

/// Response type for analytics dashboard stats.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub visitors: u64,
    pub pageviews: u64,
    pub bounce_rate: f64,
    pub avg_visit_duration: f64,
    pub prev_visitors: Option<u64>,
    pub prev_pageviews: Option<u64>,
}

/// A single row in a breakdown table (top pages, referrers, etc.).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BreakdownRow {
    pub name: String,
    pub visitors: u64,
    pub pageviews: u64,
}

/// Time series data point for the visitors chart.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TimeseriesPoint {
    pub date: String,
    pub visitors: u64,
    pub pageviews: u64,
}

/// Request to create a new source.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateSourceRequest {
    pub domain: String,
    pub name: String,
    #[serde(default)]
    pub timezone: Option<String>,
}

/// Request to update a source.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceRequest {
    pub name: Option<String>,
    pub timezone: Option<String>,
}

//! Event, source, and profile shapes for ingestion — both the raw payloads
//! accepted from the tracking script and the enriched rows written to Parquet.
//!
//! The `Raw*` types mirror what a browser sends and are deliberately permissive;
//! `Event` is what has survived validation and enrichment. Keeping them separate
//! is what stops unvalidated client fields from reaching storage.
//!
//! `Event` carries no raw IP or user-agent, only values derived from them
//! (`browser`, `os`, `device_type`, and the hashed `visitor_id`). That is the
//! privacy guarantee made concrete — adding either field here would undo it, so
//! don't, however convenient it looks for debugging.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A registered website/domain that sends analytics events.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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
    /// Known user ID (optional — empty when anonymous).
    #[serde(default)]
    pub user_id: Option<String>,
}

/// Enriched event (after processing). All fields are strings for flat Parquet columns.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    // Envelope
    pub id: String,
    pub timestamp: String,
    pub source_id: String,
    pub event_name: String,

    // Identity
    pub visitor_id: String,
    pub session_id: String,
    pub user_id: String,

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

// ── Product Analytics Types ──────────────────────────────────────

/// Incoming track event from product analytics (server-side or client-side).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawTrackEvent {
    /// Event name (e.g. "signup", "checkout").
    pub name: String,
    /// Known user ID (optional).
    #[serde(default)]
    pub user_id: Option<String>,
    /// Source domain.
    pub domain: String,
    /// Page URL (optional for server-side events).
    #[serde(default)]
    pub url: Option<String>,
    /// Custom properties.
    #[serde(default)]
    pub props: Option<serde_json::Value>,
}

/// Incoming identify call — associates traits with a user.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIdentifyEvent {
    /// The user ID to identify.
    pub user_id: String,
    /// Source domain.
    pub domain: String,
    /// Traits to store on the user profile (e.g. name, email, plan).
    #[serde(default)]
    pub traits: HashMap<String, serde_json::Value>,
}

/// User profile stored in the ingest database.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user_id: String,
    pub source_id: String,
    /// JSON-encoded traits.
    pub traits: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A single step in a funnel definition.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FunnelStep {
    pub name: String,
}

/// Result for one step in a funnel.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FunnelResult {
    pub steps: Vec<FunnelStepResult>,
}

/// A single step result in a funnel.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FunnelStepResult {
    pub name: String,
    pub count: u64,
    pub conversion_rate: f64,
    pub dropoff_rate: f64,
}

/// A single row in a retention matrix.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RetentionRow {
    pub cohort: String,
    pub cohort_size: u64,
    pub periods: Vec<f64>,
}

/// Full retention analysis result.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RetentionResult {
    pub period_type: String,
    pub rows: Vec<RetentionRow>,
}

/// A row in the event list (aggregated).
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EventListRow {
    pub name: String,
    pub count: u64,
    pub unique_users: u64,
}

/// A single event in a user's timeline.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UserTimelineEvent {
    pub timestamp: String,
    pub event_name: String,
    pub page_url: String,
    pub properties: String,
}

/// Request body for funnel analysis.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FunnelRequest {
    pub steps: Vec<FunnelStep>,
    /// Time window in seconds for the funnel.
    #[serde(default = "default_funnel_window")]
    pub window_seconds: u64,
    #[serde(default = "default_period")]
    pub period: String,
    pub start: Option<String>,
    pub end: Option<String>,
}

fn default_funnel_window() -> u64 {
    86400 * 7 // 7 days
}

fn default_period() -> String {
    "30d".to_string()
}

/// Request body for retention analysis.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RetentionRequest {
    /// Event that defines the cohort (e.g. "signup").
    pub cohort_event: String,
    /// Event that counts as a return (e.g. "pageview").
    pub return_event: String,
    /// Period granularity: "week" or "month".
    #[serde(default = "default_retention_period_type")]
    pub period_type: String,
    /// Number of periods to analyze.
    #[serde(default = "default_retention_periods")]
    pub num_periods: usize,
    #[serde(default = "default_period")]
    pub period: String,
    pub start: Option<String>,
    pub end: Option<String>,
}

fn default_retention_period_type() -> String {
    "week".to_string()
}

fn default_retention_periods() -> usize {
    8
}

// Row mappings, fields matching columns by name.
brightflow_store::impl_from_row!(Source {
    id,
    domain,
    name,
    timezone,
    created_at,
    updated_at
});
brightflow_store::impl_from_row!(Event {
    id,
    timestamp,
    source_id,
    event_name,
    visitor_id,
    session_id,
    user_id,
    hostname,
    pathname,
    page_url,
    referrer,
    referrer_source,
    utm_source,
    utm_medium,
    utm_campaign,
    utm_content,
    utm_term,
    browser,
    browser_version,
    os,
    os_version,
    device_type,
    screen_size,
    country,
    region,
    city,
    properties,
});
brightflow_store::impl_from_row!(UserProfile {
    user_id,
    source_id,
    traits,
    created_at,
    updated_at
});

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Compact summary of one topic cluster, used for list views.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSummary {
    pub id: i32,
    pub name: String,
    pub size: usize,
    /// Distinctive terms (c-TF-IDF) for this cluster — terms common here and
    /// rare in other clusters.
    pub top_terms: Vec<String>,
    /// Representative document titles closest to the cluster centroid,
    /// to give the user a sense of the cluster's content.
    pub sample_titles: Vec<String>,
    /// Top GitHub labels in this cluster with their share (0..=1).
    pub top_labels: Vec<LabelBucket>,
}

/// Overview returned by `GET …/topics`.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TopicsOverview {
    /// True when artifacts exist and the embedder is loadable.
    pub ready: bool,
    /// Reason why ready=false (e.g. "embedder not configured", "no artifacts").
    #[ts(optional)]
    pub reason: Option<String>,
    #[ts(optional)]
    pub embedding_model_id: Option<String>,
    #[ts(optional)]
    pub k: Option<usize>,
    pub total_rows: usize,
    /// Unix epoch seconds.
    #[ts(optional)]
    pub fitted_at: Option<i64>,
    pub clusters: Vec<ClusterSummary>,
}

/// One issue reference returned in cluster details.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct IssueRef {
    pub id: i64,
    #[ts(optional)]
    pub number: Option<i64>,
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub html_url: Option<String>,
    /// Full body content for drawer rendering. Truncated server-side if huge.
    #[ts(optional)]
    pub body: Option<String>,
    /// Cosine similarity to the cluster centroid (or label centroid).
    pub similarity: f32,
}

/// Single bucket in a label-distribution histogram.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LabelBucket {
    pub label: String,
    pub share: f32,
}

/// One point on the cluster's per-day creation timeseries.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TimeseriesPoint {
    /// Date in YYYY-MM-DD format.
    pub date: String,
    pub count: usize,
}

/// Full detail returned by `GET …/topics/clusters/{id}`.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ClusterDetail {
    pub id: i32,
    pub name: String,
    pub size: usize,
    pub top_terms: Vec<String>,
    pub samples: Vec<IssueRef>,
    pub outliers: Vec<IssueRef>,
    pub purity: f32,
    pub label_distribution: Vec<LabelBucket>,
    pub timeseries: Vec<TimeseriesPoint>,
}

/// Body for `POST …/topics/recluster`.
#[derive(Debug, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ReclusterRequest {
    #[ts(optional)]
    pub k: Option<usize>,
}

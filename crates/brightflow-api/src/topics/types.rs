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
    /// True when `name` comes from curation (a rename or an assigned label)
    /// rather than auto-generated c-TF-IDF terms.
    #[serde(default)]
    pub curated: bool,
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
    /// Rows without a cluster: ineligible after cleaning, other-language,
    /// or trimmed as outliers. Honesty metric — was silently hidden before.
    #[serde(default)]
    pub unassigned_rows: usize,
    /// Clusters that exist but fall below the display size floor.
    #[serde(default)]
    pub hidden_clusters: usize,
    /// Curation edits that lost their cluster after a re-fit (pending review).
    #[serde(default)]
    pub orphaned_edits: usize,
    /// Language the fit covers (primary subtag), when the table has one.
    #[ts(optional)]
    pub language: Option<String>,
    /// Distribution of languages at fit time.
    #[serde(default)]
    pub language_histogram: Vec<LanguageBucket>,
    /// Unix epoch seconds.
    #[ts(optional)]
    pub fitted_at: Option<i64>,
    pub clusters: Vec<ClusterSummary>,
}

/// One language's row count in the fitted table.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LanguageBucket {
    pub language: String,
    pub count: usize,
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
    /// Fit on this language (primary subtag, e.g. "ja") instead of the
    /// dominant one.
    #[ts(optional)]
    pub language: Option<String>,
    /// Embedder id override for this fit (only `"potion-base-32M"` today).
    #[ts(optional)]
    pub embedder: Option<String>,
    /// Minimum cluster size (used by density clustering; advisory for k-means).
    #[ts(optional)]
    pub min_cluster_size: Option<usize>,
    /// Clustering algorithm: "kmeans" (default) or "hdbscan".
    #[ts(optional)]
    pub algorithm: Option<String>,
}

/// Effective enrichment settings for a table (builtin default ⊕ overrides).
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentSettingsResponse {
    pub table: String,
    pub enrichable: bool,
    pub text_columns: Vec<String>,
    pub cleaning_profile: String,
    #[ts(optional)]
    pub language_column: Option<String>,
    pub embedder: String,
    #[ts(optional)]
    pub min_cluster_size: Option<usize>,
    /// Clustering algorithm: "kmeans" or "hdbscan".
    pub algorithm: String,
    /// True when stored overrides exist for this table.
    pub has_overrides: bool,
}

/// Body for `PUT …/enrichment`. All fields optional deltas.
#[derive(Debug, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEnrichmentSettingsRequest {
    #[ts(optional)]
    pub text_columns: Option<Vec<String>>,
    /// "kmeans" | "hdbscan"
    #[ts(optional)]
    pub algorithm: Option<String>,
    #[ts(optional)]
    pub cleaning_profile: Option<String>,
    #[ts(optional)]
    pub language_column: Option<String>,
    #[ts(optional)]
    pub embedder: Option<String>,
    #[ts(optional)]
    pub min_cluster_size: Option<usize>,
}

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One intent category in a table's taxonomy.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyCategory {
    /// `number`, not ts-rs's default `bigint` for i64: the wire value is a
    /// plain JSON number, and a real BigInt would break JSON.stringify on
    /// the round trip (same rationale as the id fields in `Action`).
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    #[ts(optional)]
    pub description: Option<String>,
    /// Rows currently carrying this category.
    pub labelled_rows: usize,
    /// False when `labelled_rows` is under the engine's `MIN_LABEL_SUPPORT`, so
    /// the classifier would drop this category at fit time. The curation UI uses
    /// this to point a human at the categories that actually need work.
    pub trainable: bool,
}

/// A table's taxonomy plus the numbers a curator needs to judge it.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyOverview {
    pub categories: Vec<TaxonomyCategory>,
    /// Distinct rows carrying at least one label.
    pub labelled_rows: usize,
    pub total_rows: usize,
    /// Minimum examples a category needs before the head will train on it.
    pub min_label_support: usize,
    /// Minimum labelled rows before a head can be trained at all.
    pub min_train_rows: usize,
    /// Held-out macro-F1 of the current head, when one is fitted.
    #[ts(optional)]
    pub classifier_val_macro_f1: Option<f32>,
    /// Whether a trained head exists beside the current fit.
    pub has_classifier: bool,
}

/// One sampled ticket in the curation queue, with its current labels.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CurationDoc {
    pub row_id: String,
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub body: Option<String>,
    #[ts(optional)]
    pub html_url: Option<String>,
    /// Category names currently on this row.
    pub categories: Vec<String>,
    /// "agent" when every label was proposed by the agent and not yet touched
    /// by a human, "human" when a person has curated this row, absent when
    /// unlabelled. This is what lets a curator see what still needs review.
    #[ts(optional)]
    pub source: Option<String>,
    /// The row's format cluster — shown so a curator can see that intent and
    /// format are NOT the same thing.
    #[ts(optional)]
    pub cluster_id: Option<i64>,
}

/// The curation review queue.
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CurationQueue {
    pub docs: Vec<CurationDoc>,
    pub categories: Vec<TaxonomyCategory>,
    /// Docs the agent labelled that no human has confirmed yet.
    pub pending_review: usize,
}

/// Query for `GET …/taxonomy/queue`.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CurationQueueQuery {
    /// Max docs to return.
    #[ts(optional)]
    pub limit: Option<usize>,
    /// "unlabelled" | "agent" | "human" | "all" (default "all").
    #[ts(optional)]
    pub filter: Option<String>,
}

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
    /// Top labels/tags from the table's label column (GitHub labels,
    /// Bluesky hashtags) with their share (0..=1).
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

/// One document reference returned in cluster details (a GitHub issue, a
/// Bluesky post, …).
#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DocRef {
    /// Row identifier, stringified (issue id, at:// uri, …).
    pub id: String,
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
    pub samples: Vec<DocRef>,
    pub outliers: Vec<DocRef>,
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

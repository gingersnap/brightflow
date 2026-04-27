use std::collections::HashMap;
use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    Json,
};
use polars::prelude::*;

use brightflow_engine::embedding::topics_artifact_dir;
use brightflow_engine::enrichment::{
    fit_topics, ArtifactMeta, ClusteringArtifact, TfIdfArtifact, DEFAULT_K,
};
use tracing::info;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;
use crate::topics::types::{
    ClusterDetail, ClusterSummary, IssueRef, LabelBucket, ReclusterRequest, TimeseriesPoint,
    TopicsOverview,
};

const SAMPLES_PER_CLUSTER: usize = 10;
const OUTLIERS_PER_CLUSTER: usize = 3;
/// Number of top-similarity candidates to consider when re-ranking samples
/// by distinctive-term relevance. Bounds the cost of substring matching.
const SAMPLE_RERANK_POOL: usize = 80;
/// Weight of the term-match share relative to centroid similarity in the
/// combined ranking score. 0.0 = pure similarity (boring centroid average);
/// 1.0 = term match dominates. 0.5 lets illustrative docs win ties.
const TERM_MATCH_WEIGHT: f32 = 0.5;
/// Hide clusters with fewer members than this. Tiny clusters tend to be
/// outliers/spam from k-means empty-cluster reseeding and aren't actionable.
const MIN_CLUSTER_SIZE: usize = 50;
/// Number of top labels surfaced per cluster summary.
const TOP_LABELS_PER_CLUSTER: usize = 5;

fn workspace_root(state: &AppState) -> AppResult<PathBuf> {
    state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::root)
        .ok_or_else(|| AppError::Internal("workspace paths unavailable".to_string()))
}

async fn load_table(state: &AppState, source_id: &str, table: &str) -> AppResult<DataFrame> {
    let store = state
        .store()
        .ok_or_else(|| AppError::Internal("store unavailable".to_string()))?;
    store
        .read_table(source_id, table)
        .await
        .map_err(AppError::from)
}

/// `GET /api/sources/{source_id}/tables/{table}/topics`
pub async fn get_overview(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
) -> AppResult<Json<TopicsOverview>> {
    let root = workspace_root(&state)?;
    let dir = topics_artifact_dir(&root, &source_id, &table);

    if !ArtifactMeta::exists(&dir) {
        return Ok(Json(TopicsOverview {
            ready: false,
            reason: Some("No clusters yet — run topics fit".to_string()),
            embedding_model_id: None,
            k: None,
            total_rows: 0,
            fitted_at: None,
            clusters: Vec::new(),
        }));
    }

    let meta = ArtifactMeta::load(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
    let clustering =
        ClusteringArtifact::load(&dir).map_err(|e| AppError::Internal(e.to_string()))?;

    let df = load_table(&state, &source_id, &table).await?;

    let summaries = build_cluster_summaries(&df, &clustering)?;

    Ok(Json(TopicsOverview {
        ready: true,
        reason: None,
        embedding_model_id: Some(meta.embedding_model_id),
        k: Some(meta.k),
        total_rows: meta.total_rows,
        fitted_at: Some(meta.fitted_at),
        clusters: summaries,
    }))
}

/// `GET /api/sources/{source_id}/tables/{table}/topics/clusters/{cluster_id}`
pub async fn get_cluster_detail(
    State(state): State<AppState>,
    Path((source_id, table, cluster_id)): Path<(String, String, i32)>,
) -> AppResult<Json<ClusterDetail>> {
    let root = workspace_root(&state)?;
    let dir = topics_artifact_dir(&root, &source_id, &table);

    if !ClusteringArtifact::exists(&dir) {
        return Err(AppError::NotFound("no clusters fitted".to_string()));
    }

    let clustering =
        ClusteringArtifact::load(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
    let tfidf = TfIdfArtifact::load(&dir).ok();

    if cluster_id < 0 || (cluster_id as usize) >= clustering.centroids.len() {
        return Err(AppError::NotFound(format!(
            "cluster {cluster_id} out of range"
        )));
    }
    let cid = cluster_id as usize;

    let df = load_table(&state, &source_id, &table).await?;

    let detail = build_cluster_detail(&df, &clustering, cid, tfidf.as_ref())?;
    Ok(Json(detail))
}

/// `POST /api/sources/{source_id}/tables/{table}/topics/recluster`
pub async fn post_recluster(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
    Json(body): Json<ReclusterRequest>,
) -> AppResult<Json<TopicsOverview>> {
    let root = workspace_root(&state)?;
    let k = body.k.unwrap_or(DEFAULT_K);

    let df = load_table(&state, &source_id, &table).await?;

    let root_for_task = root.clone();
    let source_owned = source_id.clone();
    let table_owned = table.clone();
    let t = std::time::Instant::now();
    let (enriched, _outcome) = tokio::task::spawn_blocking(move || {
        fit_topics(&root_for_task, &source_owned, &table_owned, &df, k)
    })
    .await?
    .map_err(|e| AppError::Analysis(e.to_string()))?;
    info!("fit_topics returned in {:?}", t.elapsed());

    // Re-ingest the enriched table.
    if let Some(store) = state.store() {
        let store_path = state
            .paths
            .as_ref()
            .map(brightflow_core::WorkspacePaths::store)
            .ok_or_else(|| AppError::Internal("paths missing".to_string()))?;
        let table_dir = store_path.join(&source_id).join(&table);
        std::fs::create_dir_all(&table_dir).map_err(AppError::Io)?;
        let output_path = table_dir.join("enriched.parquet");
        let t = std::time::Instant::now();
        {
            let file = std::fs::File::create(&output_path).map_err(AppError::Io)?;
            ParquetWriter::new(file)
                .finish(&mut enriched.clone())
                .map_err(AppError::Polars)?;
        }
        info!("Wrote enriched parquet in {:?}", t.elapsed());
        let t = std::time::Instant::now();
        store
            .ingest_parquet(
                &source_id,
                &table,
                &output_path,
                Some(brightflow_store::IngestOptions {
                    mode: brightflow_store::IngestMode::Overwrite,
                    ..Default::default()
                }),
            )
            .await?;
        info!("Ingested into store in {:?}", t.elapsed());
        if output_path.exists() {
            drop(std::fs::remove_file(&output_path));
        }
    }

    // Reuse get_overview to return the same shape
    get_overview(State(state), Path((source_id, table))).await
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_cluster_id_column(df: &DataFrame) -> AppResult<Vec<Option<i32>>> {
    df.column("topic_cluster_id")
        .map_err(|_| AppError::Internal("topic_cluster_id column missing".to_string()))?
        .as_materialized_series()
        .i32()
        .map(|ca| ca.into_iter().collect())
        .map_err(|e| AppError::Internal(format!("topic_cluster_id type: {e}")))
}

fn read_label_names_column(df: &DataFrame) -> Vec<Option<String>> {
    df.column("label_names")
        .ok()
        .and_then(|c| {
            c.as_materialized_series()
                .str()
                .ok()
                .map(|ca| ca.into_iter().map(|o| o.map(str::to_string)).collect())
        })
        .unwrap_or_else(|| vec![None; df.height()])
}

fn top_labels_for(
    counts: Option<&HashMap<String, usize>>,
    cluster_size: usize,
    top_n: usize,
) -> Vec<LabelBucket> {
    let Some(map) = counts else {
        return Vec::new();
    };
    if cluster_size == 0 {
        return Vec::new();
    }
    let mut entries: Vec<(&String, &usize)> = map.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    entries
        .into_iter()
        .take(top_n)
        .map(|(label, &count)| LabelBucket {
            label: label.clone(),
            share: (count as f32) / (cluster_size as f32),
        })
        .collect()
}

fn build_cluster_summaries(
    df: &DataFrame,
    clustering: &ClusteringArtifact,
) -> AppResult<Vec<ClusterSummary>> {
    let cluster_ids = read_cluster_id_column(df)?;
    let labels = read_label_names_column(df);
    let mut counts = vec![0_usize; clustering.centroids.len()];
    let mut label_counts: Vec<HashMap<String, usize>> = (0..clustering.centroids.len())
        .map(|_| HashMap::new())
        .collect();

    for (i, opt) in cluster_ids.iter().enumerate() {
        if let &Some(cid) = opt {
            let cid_us = cid as usize;
            if cid_us < counts.len() {
                counts[cid_us] += 1;
                if let Some(raw) = labels.get(i).and_then(Option::as_deref) {
                    for label in raw.split(',') {
                        let trimmed = label.trim();
                        if !trimmed.is_empty() {
                            *label_counts[cid_us].entry(trimmed.to_string()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }

    let summaries = clustering
        .names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let size = counts.get(i).copied().unwrap_or(0);
            if size < MIN_CLUSTER_SIZE {
                return None;
            }
            // Prefer per-cluster lists from the artifact (post c-TF-IDF refit);
            // fall back to splitting the legacy display name.
            let top_terms = clustering
                .top_terms
                .get(i)
                .cloned()
                .unwrap_or_else(|| name.split(", ").map(str::to_string).collect());
            let sample_titles = clustering.sample_titles.get(i).cloned().unwrap_or_default();
            let top_labels = top_labels_for(label_counts.get(i), size, TOP_LABELS_PER_CLUSTER);

            Some(ClusterSummary {
                id: i32::try_from(i).unwrap_or(0),
                name: name.clone(),
                size,
                top_terms,
                sample_titles,
                top_labels,
            })
        })
        .collect::<Vec<_>>();
    let mut summaries = summaries;
    summaries.sort_by(|a, b| b.size.cmp(&a.size));

    Ok(summaries)
}

fn build_cluster_detail(
    df: &DataFrame,
    clustering: &ClusteringArtifact,
    cluster_id: usize,
    _tfidf: Option<&TfIdfArtifact>,
) -> AppResult<ClusterDetail> {
    let cluster_ids = read_cluster_id_column(df)?;

    // Indices belonging to this cluster.
    let member_indices: Vec<usize> = cluster_ids
        .iter()
        .enumerate()
        .filter_map(|(i, opt)| match opt {
            Some(c) if *c as usize == cluster_id => Some(i),
            _ => None,
        })
        .collect();

    let centroid = clustering
        .centroids
        .get(cluster_id)
        .ok_or_else(|| AppError::NotFound("cluster centroid missing".to_string()))?;
    let name = clustering
        .names
        .get(cluster_id)
        .cloned()
        .unwrap_or_else(|| format!("cluster_{cluster_id}"));

    // Read embedding column once into Vec<Option<Vec<f32>>>.
    let emb_col = df
        .column("embedding")
        .map_err(|_| AppError::Internal("embedding column missing".to_string()))?
        .as_materialized_series()
        .clone();
    let emb_list = emb_col
        .list()
        .map_err(|e| AppError::Internal(format!("embedding not List<f32>: {e}")))?;
    let mut all_embeddings: Vec<Option<Vec<f32>>> = Vec::with_capacity(emb_list.len());
    #[allow(clippy::explicit_into_iter_loop)]
    for opt_s in emb_list.into_iter() {
        match opt_s {
            None => all_embeddings.push(None),
            Some(s) => match s.f32() {
                Ok(ca) => all_embeddings.push(Some(ca.into_no_null_iter().collect())),
                Err(_) => all_embeddings.push(None),
            },
        }
    }

    // Compute (index, similarity) for cluster members.
    let mut sims: Vec<(usize, f32)> = Vec::with_capacity(member_indices.len());
    let mut purity_acc = 0.0_f64;
    let mut purity_count = 0_usize;
    for &i in &member_indices {
        let Some(Some(v)) = all_embeddings.get(i) else {
            continue;
        };
        if v.len() != centroid.len() {
            continue;
        }
        let sim: f32 = v
            .iter()
            .zip(centroid.iter())
            .fold(0.0_f32, |acc, (x, y)| x.mul_add(*y, acc));
        sims.push((i, sim));
        purity_acc += f64::from(sim);
        purity_count += 1;
    }

    let purity = if purity_count == 0 {
        0.0
    } else {
        (purity_acc / purity_count as f64) as f32
    };

    sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top_terms: Vec<String> = clustering
        .top_terms
        .get(cluster_id)
        .cloned()
        .unwrap_or_else(|| name.split(", ").map(str::to_string).collect());

    // Re-rank top similarity candidates by how well they showcase the
    // distinctive terms. Combined score = similarity + α × term_match_share,
    // so similarity stays primary but illustrative docs bubble up.
    let samples_idx = pick_illustrative_samples(
        df,
        &sims,
        &top_terms,
        SAMPLES_PER_CLUSTER,
        SAMPLE_RERANK_POOL,
    );
    let outliers_idx: Vec<(usize, f32)> = sims
        .iter()
        .rev()
        .take(OUTLIERS_PER_CLUSTER)
        .copied()
        .collect();

    let samples = sims_to_refs(df, &samples_idx);
    let outliers = sims_to_refs(df, &outliers_idx);

    let label_distribution = build_label_distribution(df, &member_indices);
    let timeseries = build_timeseries(df, &member_indices);

    Ok(ClusterDetail {
        id: i32::try_from(cluster_id).unwrap_or(0),
        name,
        size: member_indices.len(),
        top_terms,
        samples,
        outliers,
        purity,
        label_distribution,
        timeseries,
    })
}

fn read_string_at(df: &DataFrame, col: &str, row: usize) -> Option<String> {
    df.column(col)
        .ok()?
        .as_materialized_series()
        .str()
        .ok()?
        .get(row)
        .map(str::to_string)
}

fn read_i64_at(df: &DataFrame, col: &str, row: usize) -> Option<i64> {
    let series = df.column(col).ok()?.as_materialized_series();
    if let Ok(ca) = series.i64() {
        return ca.get(row);
    }
    if let Ok(ca) = series.i32() {
        return ca.get(row).map(i64::from);
    }
    None
}

const BODY_TRUNCATE_CHARS: usize = 8000;

/// Re-rank a similarity-sorted candidate list to favor docs that contain the
/// cluster's distinctive terms. Score = sim + α × (terms_present / total_terms).
fn pick_illustrative_samples(
    df: &DataFrame,
    sims_desc: &[(usize, f32)],
    distinctive_terms: &[String],
    take: usize,
    pool: usize,
) -> Vec<(usize, f32)> {
    if distinctive_terms.is_empty() || take == 0 {
        return sims_desc.iter().take(take).copied().collect();
    }
    let lower_terms: Vec<String> = distinctive_terms
        .iter()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect();
    if lower_terms.is_empty() {
        return sims_desc.iter().take(take).copied().collect();
    }

    let mut scored: Vec<(usize, f32, f32)> = sims_desc
        .iter()
        .take(pool)
        .map(|&(i, sim)| {
            let title = read_string_at(df, "title", i)
                .unwrap_or_default()
                .to_lowercase();
            let body = read_string_at(df, "body", i)
                .unwrap_or_default()
                .to_lowercase();
            let matches = lower_terms
                .iter()
                .filter(|t| title.contains(t.as_str()) || body.contains(t.as_str()))
                .count();
            #[allow(clippy::cast_precision_loss)]
            let term_share = (matches as f32) / (lower_terms.len() as f32);
            let combined = TERM_MATCH_WEIGHT.mul_add(term_share, sim);
            (i, sim, combined)
        })
        .collect();
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(take)
        .map(|(i, sim, _)| (i, sim))
        .collect()
}

fn sims_to_refs(df: &DataFrame, indices: &[(usize, f32)]) -> Vec<IssueRef> {
    let mut out = Vec::with_capacity(indices.len());
    for &(i, sim) in indices {
        let id = read_i64_at(df, "id", i).unwrap_or(0);
        let number = read_i64_at(df, "number", i);
        let title = read_string_at(df, "title", i);
        let html_url = read_string_at(df, "html_url", i);
        let body = read_string_at(df, "body", i).map(|s| {
            if s.chars().count() > BODY_TRUNCATE_CHARS {
                let head: String = s.chars().take(BODY_TRUNCATE_CHARS).collect();
                format!("{head}…")
            } else {
                s
            }
        });
        out.push(IssueRef {
            id,
            number,
            title,
            html_url,
            body,
            similarity: sim,
        });
    }
    out
}

fn build_label_distribution(df: &DataFrame, member_indices: &[usize]) -> Vec<LabelBucket> {
    let labels = read_label_names_column(df);
    let mut counts: HashMap<String, usize> = HashMap::new();
    let cluster_size = member_indices.len();
    for &i in member_indices {
        if let Some(raw) = labels.get(i).and_then(Option::as_deref) {
            for label in raw.split(',') {
                let trimmed = label.trim();
                if !trimmed.is_empty() {
                    *counts.entry(trimmed.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    if cluster_size == 0 {
        return Vec::new();
    }
    let mut buckets: Vec<LabelBucket> = counts
        .into_iter()
        .map(|(label, c)| LabelBucket {
            label,
            share: (c as f32) / (cluster_size as f32),
        })
        .collect();
    buckets.sort_by(|a, b| {
        b.share
            .partial_cmp(&a.share)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    buckets
}

fn build_timeseries(df: &DataFrame, member_indices: &[usize]) -> Vec<TimeseriesPoint> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for &i in member_indices {
        if let Some(s) = read_string_at(df, "created_at", i) {
            let date = s.get(..10).map_or_else(|| s.clone(), str::to_string);
            *counts.entry(date).or_insert(0) += 1;
        }
    }
    let mut points: Vec<TimeseriesPoint> = counts
        .into_iter()
        .map(|(date, count)| TimeseriesPoint { date, count })
        .collect();
    points.sort_by(|a, b| a.date.cmp(&b.date));
    points
}

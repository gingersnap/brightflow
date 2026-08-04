//! Read endpoints for the intent taxonomy and the curation queue.
//!
//! Writes deliberately do NOT live here — they go through `POST /api/actions`
//! (`define_taxonomy_category`, `rename_taxonomy_category`,
//! `delete_taxonomy_category`, `label_document`) so a human edit and an agent
//! proposal share one execution path, one audit log and one undo story. These
//! handlers only read.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::Json;

use brightflow_engine::embedding::topics_artifact_dir;
use brightflow_engine::enrichment::ArtifactMeta;
use brightflow_engine::nlp::linear::{min_examples_per_label, min_labelled_rows_to_train};

use crate::shared::{AppError, AppResult};
use crate::state::AppState;
use crate::topics::types::{
    CurationDoc, CurationQueue, CurationQueueQuery, TaxonomyCategory, TaxonomyOverview,
};

/// Default page size for the curation queue.
const DEFAULT_QUEUE_LIMIT: usize = 50;
const MAX_QUEUE_LIMIT: usize = 500;

/// Resolve (store, table_id) or 404.
async fn table_id(state: &AppState, source_id: &str, table: &str) -> AppResult<String> {
    let store = state.require_store()?;
    let row = store
        .db()
        .get_table(source_id, table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Table '{table}' not found")))?;
    Ok(row.id)
}

/// Build category views with their support counts.
async fn categories_with_support(
    state: &AppState,
    table_id: &str,
) -> AppResult<Vec<TaxonomyCategory>> {
    let store = state.require_store()?;
    let rows = store.db().get_taxonomy_categories(table_id).await?;
    let counts: HashMap<i64, i64> = store
        .db()
        .count_labels_per_category(table_id)
        .await?
        .into_iter()
        .collect();

    // Judge trainability against the number that actually trains, not the raw
    // MIN_LABEL_SUPPORT constant — support is counted on the training split, so
    // the full-set bar is higher. Reporting the constant would tell a curator
    // they were done while the head silently dropped the category.
    let required = min_examples_per_label();
    Ok(rows
        .into_iter()
        .map(|c| {
            let n = counts.get(&c.id).copied().unwrap_or(0).max(0);
            let labelled_rows = usize::try_from(n).unwrap_or(0);
            TaxonomyCategory {
                id: c.id,
                name: c.name,
                description: c.description,
                labelled_rows,
                trainable: labelled_rows >= required,
            }
        })
        .collect())
}

/// `GET /api/sources/{source_id}/tables/{table}/taxonomy`
pub async fn get_taxonomy(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
) -> AppResult<Json<TaxonomyOverview>> {
    let tid = table_id(&state, &source_id, &table).await?;
    let store = state.require_store()?;

    let categories = categories_with_support(&state, &tid).await?;
    let labelled = store.db().count_labelled_rows(&tid).await?;
    let total_rows = store
        .read_table(&source_id, &table)
        .await
        .map_or(0, |df| df.height());

    // The head's own reported F1 — the claim, checkable in prod. Missing or
    // unreadable artifacts degrade to "no classifier", never a 500.
    let meta = state
        .paths
        .as_ref()
        .map(brightflow_core::WorkspacePaths::root)
        .map(|root| topics_artifact_dir(&root, &source_id, &table))
        .and_then(|dir| ArtifactMeta::load(&dir).ok());

    Ok(Json(TaxonomyOverview {
        categories,
        labelled_rows: usize::try_from(labelled.max(0)).unwrap_or(0),
        total_rows,
        min_label_support: min_examples_per_label(),
        min_train_rows: min_labelled_rows_to_train(),
        classifier_val_macro_f1: meta.as_ref().and_then(|m| m.classifier_val_macro_f1),
        has_classifier: meta.is_some_and(|m| m.has_classifier),
    }))
}

/// `GET /api/sources/{source_id}/tables/{table}/taxonomy/queue`
///
/// The review surface: sampled tickets with their proposed labels. Uses the
/// same stratified sample the labelling agent saw, so a curator reviews exactly
/// the rows the agent was asked about.
pub async fn get_curation_queue(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
    Query(query): Query<CurationQueueQuery>,
) -> AppResult<Json<CurationQueue>> {
    let tid = table_id(&state, &source_id, &table).await?;
    let store = state.require_store()?;

    let limit = query
        .limit
        .unwrap_or(DEFAULT_QUEUE_LIMIT)
        .clamp(1, MAX_QUEUE_LIMIT);
    let filter = query.filter.as_deref().unwrap_or("all");

    // Label lookup: row_id -> (names, sources).
    let mut by_row: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    for label in store.db().get_document_labels(&tid).await? {
        let entry = by_row.entry(label.row_id).or_default();
        entry.0.push(label.name);
        entry.1.push(label.source);
    }
    let pending_review = by_row
        .values()
        .filter(|(_, sources)| sources.iter().all(|s| s == "agent"))
        .count();

    // Sample from the same stratified pool the agent labelled. Ask for more
    // than `limit` because filtering happens after sampling.
    let sample = crate::agent::sampling::stratified_sample(
        &state,
        &source_id,
        &table,
        crate::agent::sampling::LABEL_SAMPLE,
    )
    .await?;

    let docs: Vec<CurationDoc> = sample
        .into_iter()
        .filter_map(|d| {
            let labelled = by_row.get(&d.row_id);
            let source = labelled.map(|(_, sources)| {
                if sources.iter().all(|s| s == "agent") {
                    "agent".to_string()
                } else {
                    "human".to_string()
                }
            });
            let keep = match filter {
                "unlabelled" => labelled.is_none(),
                "agent" => source.as_deref() == Some("agent"),
                "human" => source.as_deref() == Some("human"),
                _ => true,
            };
            if !keep {
                return None;
            }
            Some(CurationDoc {
                row_id: d.row_id,
                title: d.title,
                body: d.body,
                html_url: None,
                categories: labelled.map(|(names, _)| names.clone()).unwrap_or_default(),
                source,
                cluster_id: d.cluster_id,
            })
        })
        .take(limit)
        .collect();

    Ok(Json(CurationQueue {
        docs,
        categories: categories_with_support(&state, &tid).await?,
        pending_review,
    }))
}

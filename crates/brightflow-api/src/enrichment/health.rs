//! Read endpoints for the two numbers the two-call design prices and judges
//! on: tokens per source language, and vocabulary health per level.
//!
//! Both read the materialised table rather than the cache alone: language
//! and the chosen category live on the row, tokens live on the cell, and the
//! input hash joins them.

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::extract::{Path, State};
use axum::Json;
use polars::prelude::*;

use brightflow_engine::enrichment::ticket_classify::{LANGUAGE_INPUT, OUTPUT_COLUMNS};
use brightflow_engine::enrichment::{health, is_other, VocabKind};

use super::runner::prepare_run_inputs;
use super::types::{
    CategoryCount, TicketSummaryResponse, UsageByLanguageResponse, UsageByLanguageRow, ValueCount,
    VocabularyHealthResponse, VocabularyLevelHealth, VocabularyParentHealth,
};
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Value used when a row has no detectable language.
const UNKNOWN_LANGUAGE: &str = "unknown";

fn str_column(df: &DataFrame, name: &str) -> Option<Vec<Option<String>>> {
    let s = df
        .column(name)
        .ok()?
        .as_materialized_series()
        .str()
        .ok()?
        .clone();
    Some(s.into_iter().map(|o| o.map(str::to_string)).collect())
}

/// `GET /api/functions/{id}/usage-by-language`
///
/// Token totals grouped by the row's language: the pre-call detector's value
/// for `ticket_classify`, the table's `language` column otherwise. Rows
/// sharing an input hash share one cell, so cells are counted once per
/// language and rows separately.
pub async fn usage_by_language(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<UsageByLanguageResponse>> {
    let store = state.require_store()?;
    let row = store
        .db()
        .get_enrichment_function(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("function {id} not found")))?;
    let table = store
        .db()
        .get_table_by_id(&row.table_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table for function {id} not found")))?;
    let version = store
        .db()
        .get_enrichment_function_version(&row.id, row.current_version)
        .await?
        .ok_or_else(|| AppError::Internal("missing version snapshot".to_string()))?;
    let spec = serde_json::from_str(&version.config_json)
        .map_err(|e| AppError::Internal(format!("stored config unreadable: {e}")))?;
    let run = super::vocab::run_spec_for(store, &row.table_id, spec).await?;

    let df = store.read_table(&table.source_id, &table.name).await?;
    let inputs = prepare_run_inputs(&df, &run)?;
    let table_language = str_column(&df, OUTPUT_COLUMNS[1]);
    let mut hashes: Vec<String> = inputs.iter().map(|r| r.hash.clone()).collect();
    hashes.sort_unstable();
    hashes.dedup();
    let cells: HashMap<String, brightflow_store::EnrichmentCacheRow> = store
        .db()
        .get_cached_cells(&row.id, &run.spec_hash(), &hashes)
        .await?
        .into_iter()
        .map(|c| (c.input_hash.clone(), c))
        .collect();

    let mut by_language: BTreeMap<String, UsageByLanguageRow> = BTreeMap::new();
    let mut counted: HashSet<&str> = HashSet::new();
    for (i, input) in inputs.iter().enumerate() {
        let language = input
            .rendered
            .get(LANGUAGE_INPUT)
            .cloned()
            .filter(|l| !l.is_empty())
            .or_else(|| {
                table_language
                    .as_ref()
                    .and_then(|c| c.get(i).cloned().flatten())
            })
            .unwrap_or_else(|| UNKNOWN_LANGUAGE.to_string());
        let entry = by_language
            .entry(language.clone())
            .or_insert_with(|| UsageByLanguageRow {
                language,
                rows: 0,
                cells: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                cached_tokens: 0,
            });
        entry.rows += 1;
        if let Some(cell) = cells.get(&input.hash) {
            if counted.insert(input.hash.as_str()) {
                entry.cells += 1;
                entry.prompt_tokens += cell.prompt_tokens.unwrap_or(0);
                entry.completion_tokens += cell.completion_tokens.unwrap_or(0);
                entry.cached_tokens += cell.cached_tokens.unwrap_or(0);
            }
        }
    }
    Ok(Json(UsageByLanguageResponse {
        function_id: row.id,
        languages: by_language.into_values().collect(),
    }))
}

fn count_values(values: &[Option<String>]) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for v in values.iter().flatten() {
        *counts.entry(v.clone()).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

/// `GET /api/sources/{source_id}/tables/{table}/vocabulary/health`
///
/// Per level: entry count vs cap, other-rate, and the balance band, from the
/// materialised `category` / `subcategory` columns. Subcategories are judged
/// per parent, since that is the level a curator reviews.
pub async fn vocabulary_health(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
) -> AppResult<Json<VocabularyHealthResponse>> {
    let store = state.require_store()?;
    let table_row = store
        .db()
        .get_table(&source_id, &table)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("table '{table}' not found")))?;
    let df = store.read_table(&source_id, &table).await?;

    let categories = str_column(&df, OUTPUT_COLUMNS[2]);
    let subcategories = str_column(&df, OUTPUT_COLUMNS[3]);
    let mut levels = Vec::new();
    let mut per_parent = Vec::new();

    if let Some(cats) = &categories {
        let defined = store
            .db()
            .list_vocabulary(&table_row.id, VocabKind::Category.as_str(), 0)
            .await?;
        let mut counts = count_values(cats);
        // Defined-but-unused entries still show, at zero: that is a finding.
        for d in &defined {
            if !counts.iter().any(|(v, _)| v == &d.name) {
                counts.push((d.name.clone(), 0));
            }
        }
        levels.push(VocabularyLevelHealth::from(health(
            VocabKind::Category,
            &counts,
        )));

        if let Some(subs) = &subcategories {
            let mut by_parent: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
            for (c, s) in cats.iter().zip(subs.iter()) {
                if let Some(c) = c {
                    by_parent.entry(c.clone()).or_default().push(s.clone());
                }
            }
            for (parent, values) in by_parent {
                // `other` has no row; its subcategories sit at OTHER_PARENT.
                let parent_id = if is_other(&parent) {
                    Some(brightflow_engine::enrichment::OTHER_PARENT)
                } else {
                    defined.iter().find(|d| d.name == parent).map(|d| d.id)
                };
                let mut child_counts = count_values(&values);
                if let Some(pid) = parent_id {
                    for d in store
                        .db()
                        .list_vocabulary(&table_row.id, VocabKind::Subcategory.as_str(), pid)
                        .await?
                    {
                        if !child_counts.iter().any(|(v, _)| v == &d.name) {
                            child_counts.push((d.name.clone(), 0));
                        }
                    }
                }
                per_parent.push(VocabularyParentHealth {
                    parent,
                    health: VocabularyLevelHealth::from(health(
                        VocabKind::Subcategory,
                        &child_counts,
                    )),
                });
            }
        }
    }

    Ok(Json(VocabularyHealthResponse {
        classified_rows: categories
            .as_ref()
            .map_or(0, |c| c.iter().flatten().count()),
        total_rows: df.height(),
        levels,
        per_parent,
        induction_min_rows: crate::agent::runner::MIN_SUMMARIES_FOR_INDUCTION,
        min_rows_per_entry: brightflow_engine::enrichment::MIN_ROWS_PER_ENTRY,
    }))
}

fn value_counts(values: &[Option<String>]) -> Vec<ValueCount> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for v in values.iter().flatten() {
        *counts.entry(v.clone()).or_insert(0) += 1;
    }
    let mut out: Vec<ValueCount> = counts
        .into_iter()
        .map(|(value, rows)| ValueCount { value, rows })
        .collect();
    out.sort_by(|a, b| b.rows.cmp(&a.rows).then_with(|| a.value.cmp(&b.value)));
    out
}

/// `GET /api/sources/{source_id}/tables/{table}/tickets/summary`
///
/// Row grain: rows per category (with its subcategories), per sentiment,
/// and per language, from the classifier's materialised columns.
/// "billing tickets up 12 %" starts here; the time axis is the insights
/// engine's job once these columns exist.
pub async fn ticket_summary(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
) -> AppResult<Json<TicketSummaryResponse>> {
    let store = state.require_store()?;
    let df = store.read_table(&source_id, &table).await?;
    let categories = str_column(&df, OUTPUT_COLUMNS[2]).unwrap_or_default();
    let subcategories = str_column(&df, OUTPUT_COLUMNS[3]).unwrap_or_default();
    let sentiment = str_column(&df, OUTPUT_COLUMNS[4]).unwrap_or_default();
    let language = str_column(&df, OUTPUT_COLUMNS[1]).unwrap_or_default();

    let mut per_category: BTreeMap<String, (usize, Vec<Option<String>>)> = BTreeMap::new();
    for (i, cat) in categories.iter().enumerate() {
        let Some(cat) = cat else { continue };
        let entry = per_category
            .entry(cat.clone())
            .or_insert_with(|| (0, Vec::new()));
        entry.0 += 1;
        entry.1.push(subcategories.get(i).cloned().flatten());
    }
    let mut category_rows: Vec<CategoryCount> = per_category
        .into_iter()
        .map(|(category, (rows, subs))| CategoryCount {
            category,
            rows,
            subcategories: value_counts(&subs),
        })
        .collect();
    category_rows.sort_by(|a, b| {
        b.rows
            .cmp(&a.rows)
            .then_with(|| a.category.cmp(&b.category))
    });

    Ok(Json(TicketSummaryResponse {
        total_rows: df.height(),
        classified_rows: categories.iter().flatten().count(),
        categories: category_rows,
        sentiment: value_counts(&sentiment),
        languages: value_counts(&language),
    }))
}

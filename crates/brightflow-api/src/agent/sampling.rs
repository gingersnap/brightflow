//! Stratified corpus sampling for the intent-taxonomy agents.
//!
//! # Why stratify on format clusters
//!
//! The existing `topic_cluster_id` assignments are format-shaped — that is the
//! whole problem this feature exists to fix. They are nonetheless the best
//! *sampling device* available: we need the seed sample to span the corpus, and
//! what we want from clusters here is **diversity, not correctness**. Drawing
//! evenly across format strata guarantees the LLM sees backport chatter,
//! stack traces and templated reports alike, instead of 500 near-identical
//! tickets from whatever format happens to dominate.
//!
//! Rows with no cluster (trimmed outliers) are sampled too — they are exactly
//! the rows the current pipeline has nothing to say about.

use polars::prelude::*;
use std::collections::BTreeMap;

use brightflow_engine::nlp::SplitMix64;

use crate::shared::{AppError, AppResult};
use crate::state::AppState;
use crate::topics::display::DocDisplay;
use crate::topics::labels::read_row_ids;

/// Documents shown to `propose_taxonomy`. Small: the model only has to infer a
/// vocabulary, and the whole point is that LLM cost stays O(taxonomy), not
/// O(rows).
pub const TAXONOMY_SAMPLE: usize = 60;
/// Summaries shown to the summary-driven induction runs.
///
/// Larger than `TAXONOMY_SAMPLE` because a summary is ~12 tokens: 200 of
/// them cost less than 60 truncated bodies and are a far better induction
/// corpus.
pub const INDUCTION_SAMPLE: usize = 200;
/// Documents labelled by `label_documents` — the seed the head trains on.
pub const LABEL_SAMPLE: usize = 1_200;
/// Rows per `label_document` batch shown to the model at once.
pub const LABEL_BATCH: usize = 25;

/// Body text is truncated hard: a handful of long tickets would otherwise eat
/// the context window and starve the sample of diversity.
const TITLE_MAX: usize = 160;
const BODY_MAX: usize = 400;

/// Deterministic — the same table always yields the same seed sample, so a
/// re-run resumes the same curation queue instead of reshuffling it.
const SAMPLE_SEED: u64 = 0x00_5a_11_9e_50_00_00_01;

/// One sampled document, trimmed for prompting.
#[derive(Debug, Clone)]
pub struct SampleDoc {
    pub row_id: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub cluster_id: Option<i64>,
}

impl SampleDoc {
    /// Compact JSON for a prompt.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "rowId": self.row_id,
            "title": self.title,
            "body": self.body,
        })
    }
}

fn truncate(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(max).collect();
    format!("{cut}…")
}

/// Read a string column into per-row options.
fn read_str_column(df: &DataFrame, col: &str) -> Option<Vec<Option<String>>> {
    let s = df
        .column(col)
        .ok()?
        .as_materialized_series()
        .str()
        .ok()?
        .clone();
    Some(s.into_iter().map(|o| o.map(str::to_string)).collect())
}

fn read_cluster_ids(df: &DataFrame) -> Vec<Option<i64>> {
    df.column("topic_cluster_id")
        .ok()
        .and_then(|c| c.as_materialized_series().cast(&DataType::Int64).ok())
        .and_then(|s| s.i64().ok().cloned())
        .map_or_else(
            || vec![None; df.height()],
            |ca| ca.into_iter().collect::<Vec<Option<i64>>>(),
        )
}

/// Draw up to `n` documents spread evenly across format strata.
pub async fn stratified_sample(
    state: &AppState,
    source_id: &str,
    table: &str,
    n: usize,
) -> AppResult<Vec<SampleDoc>> {
    let store = state.require_store()?;
    let df = store.read_table(source_id, table).await?;
    let display = DocDisplay::for_table(table);

    let ids = read_row_ids(&df, display.id_column).ok_or_else(|| {
        AppError::BadRequest(format!(
            "table '{table}' has no readable id column '{}'",
            display.id_column
        ))
    })?;
    let titles = display.title_column.and_then(|c| read_str_column(&df, c));
    let bodies = display.body_column.and_then(|c| read_str_column(&df, c));
    let clusters = read_cluster_ids(&df);

    // Bucket rows by stratum. BTreeMap keyed by Option<i64> so iteration order
    // is deterministic (None — the outliers — sorts first and is never starved).
    let mut buckets: BTreeMap<Option<i64>, Vec<usize>> = BTreeMap::new();
    for (row, cluster) in clusters.iter().enumerate() {
        buckets.entry(*cluster).or_default().push(row);
    }
    if buckets.is_empty() {
        return Ok(Vec::new());
    }

    // Shuffle within each stratum so we take a spread, not the first N rows
    // (which would be import-ordered and therefore time-correlated).
    let mut rng = SplitMix64::new(SAMPLE_SEED);
    for rows in buckets.values_mut() {
        rng.shuffle(rows);
    }

    // Round-robin across strata until we have n. This gives small strata
    // proportionally more weight than random sampling would — deliberately:
    // a rare format is where an unnamed intent is most likely hiding.
    let mut picked: Vec<usize> = Vec::with_capacity(n.min(df.height()));
    let mut cursor = 0usize;
    while picked.len() < n {
        let mut advanced = false;
        for rows in buckets.values() {
            if picked.len() >= n {
                break;
            }
            if let Some(&row) = rows.get(cursor) {
                picked.push(row);
                advanced = true;
            }
        }
        if !advanced {
            break; // every stratum exhausted
        }
        cursor += 1;
    }

    Ok(picked
        .into_iter()
        .map(|row| SampleDoc {
            row_id: ids.get(row).cloned().unwrap_or_default(),
            title: titles
                .as_ref()
                .and_then(|t| t.get(row).cloned().flatten())
                .map(|s: String| truncate(s.as_str(), TITLE_MAX))
                .filter(|s| !s.is_empty()),
            body: bodies
                .as_ref()
                .and_then(|b| b.get(row).cloned().flatten())
                .map(|s: String| truncate(s.as_str(), BODY_MAX))
                .filter(|s| !s.is_empty()),
            cluster_id: clusters.get(row).copied().flatten(),
        })
        .filter(|d| !d.row_id.is_empty())
        .collect())
}

/// One summary for an induction run.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDoc {
    pub row_id: String,
    pub summary: String,
}

/// Uniform, seeded sample of classified ticket summaries.
///
/// Optionally only those whose `category` is `category_filter` (subcategory
/// induction). Rows without a summary are not classified yet and never
/// sampled.
pub async fn summary_sample(
    state: &AppState,
    source_id: &str,
    table: &str,
    n: usize,
    category_filter: Option<&str>,
) -> AppResult<Vec<SummaryDoc>> {
    let store = state.require_store()?;
    let df = store.read_table(source_id, table).await?;
    let display = DocDisplay::for_table(table);
    let ids = read_row_ids(&df, display.id_column).ok_or_else(|| {
        AppError::BadRequest(format!(
            "table '{table}' has no readable id column '{}'",
            display.id_column
        ))
    })?;
    let summaries = read_str_column(&df, "summary").unwrap_or_default();
    let categories = read_str_column(&df, "category");
    let candidates: Vec<usize> = (0..df.height())
        .filter(|&row| {
            summaries
                .get(row)
                .and_then(|s| s.as_deref())
                .is_some_and(|s| !s.trim().is_empty())
        })
        .filter(|&row| match (category_filter, &categories) {
            (Some(want), Some(cats)) => cats
                .get(row)
                .and_then(|c| c.as_deref())
                .is_some_and(|c| c == want),
            (Some(_), None) => false,
            (None, _) => true,
        })
        .collect();
    Ok(pick_summaries(&candidates, n, &ids, &summaries))
}

/// Seeded sample of `feedback_summary` values from the table's mention
/// child table (`{table}_mentions`); empty when it does not exist yet.
pub async fn feedback_summary_sample(
    state: &AppState,
    source_id: &str,
    table: &str,
    n: usize,
) -> AppResult<Vec<SummaryDoc>> {
    let store = state.require_store()?;
    let mentions = format!("{table}_mentions");
    let Some(_) = store.db().get_table(source_id, &mentions).await? else {
        return Ok(Vec::new());
    };
    let df = store.read_table(source_id, &mentions).await?;
    let ids = read_str_column(&df, "ticket_id")
        .or_else(|| {
            df.column("ticket_id")
                .ok()
                .and_then(|c| c.as_materialized_series().cast(&DataType::String).ok())
                .and_then(|s| s.str().ok().cloned())
                .map(|ca| ca.into_iter().map(|o| o.map(str::to_string)).collect())
        })
        .unwrap_or_default();
    let ids: Vec<String> = ids.into_iter().map(Option::unwrap_or_default).collect();
    let summaries = read_str_column(&df, "feedback_summary").unwrap_or_default();
    let candidates: Vec<usize> = (0..df.height())
        .filter(|&row| {
            summaries
                .get(row)
                .and_then(|s| s.as_deref())
                .is_some_and(|s| !s.trim().is_empty())
        })
        .collect();
    Ok(pick_summaries(&candidates, n, &ids, &summaries))
}

/// Deterministic uniform pick of up to `n` candidates (seeded shuffle, so a
/// re-run resumes the same queue).
fn pick_summaries(
    candidates: &[usize],
    n: usize,
    ids: &[String],
    summaries: &[Option<String>],
) -> Vec<SummaryDoc> {
    let mut rows = candidates.to_vec();
    let mut rng = SplitMix64::new(SAMPLE_SEED);
    rng.shuffle(&mut rows);
    rows.into_iter()
        .take(n)
        .map(|row| SummaryDoc {
            row_id: ids.get(row).cloned().unwrap_or_default(),
            summary: summaries.get(row).cloned().flatten().unwrap_or_default(),
        })
        .filter(|d| !d.row_id.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{pick_summaries, truncate};

    /// Seeded: the same candidates give the same pick; n bounds it; rows with
    /// no id are dropped.
    #[test]
    fn pick_summaries_is_deterministic_and_bounded() {
        let ids: Vec<String> = (0..10)
            .map(|i| if i == 3 { String::new() } else { i.to_string() })
            .collect();
        let summaries: Vec<Option<String>> = (0..10).map(|i| Some(format!("s{i}"))).collect();
        let candidates: Vec<usize> = (0..10).collect();
        let a = pick_summaries(&candidates, 4, &ids, &summaries);
        let b = pick_summaries(&candidates, 4, &ids, &summaries);
        assert_eq!(
            a.iter().map(|d| d.row_id.clone()).collect::<Vec<_>>(),
            b.iter().map(|d| d.row_id.clone()).collect::<Vec<_>>()
        );
        assert!(a.len() <= 4);
        assert!(a
            .iter()
            .all(|d| !d.row_id.is_empty() && d.summary.starts_with('s')));
        assert!(pick_summaries(&[], 4, &ids, &summaries).is_empty());
    }

    #[test]
    fn truncate_leaves_short_text_alone() {
        assert_eq!(truncate("  hello  ", 20), "hello");
    }

    #[test]
    fn truncate_cuts_long_text_and_marks_it() {
        let out = truncate("abcdefghij", 4);
        assert_eq!(out, "abcd…");
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        // Byte-slicing multi-byte text would panic on a char boundary.
        let out = truncate("日本語のテキストです", 3);
        assert_eq!(out, "日本語…");
    }
}

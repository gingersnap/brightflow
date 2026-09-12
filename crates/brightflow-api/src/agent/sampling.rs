//! Seeded summary sampling for the vocabulary-induction agents.
//!
//! Induction reads *summaries* (Call A output), never ticket bodies: a
//! summary is ~12 tokens, so 500 of them cost less than a few dozen
//! truncated bodies and describe what tickets are about rather than how
//! they are written. Sampling is uniform and seeded so a re-run resumes the
//! same queue instead of reshuffling it.

use polars::prelude::*;

use brightflow_engine::nlp::SplitMix64;

use crate::enrichment::display::DocDisplay;
use crate::shared::{AppError, AppResult};
use crate::state::AppState;

/// Summaries shown to the summary-driven induction runs.
pub const INDUCTION_SAMPLE: usize = 500;

/// Deterministic — the same table always yields the same sample.
const SAMPLE_SEED: u64 = 0x00_5a_11_9e_50_00_00_01;

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

/// The table's row ids as strings, whatever the id column's dtype; None
/// when the column is missing. Ids are the join key for everything that
/// references a row, so they are never taken positionally.
pub fn read_row_ids(df: &DataFrame, id_column: &str) -> Option<Vec<String>> {
    let s = df
        .column(id_column)
        .ok()?
        .as_materialized_series()
        .cast(&DataType::String)
        .ok()?;
    Some(
        s.str()
            .ok()?
            .into_iter()
            .map(|o| o.unwrap_or("").to_string())
            .collect(),
    )
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
    let columns: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(ToString::to_string)
        .collect();
    let display = DocDisplay::resolve(store, source_id, table, &columns).await?;
    let ids = read_row_ids(&df, &display.id_column).ok_or_else(|| {
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
    use super::pick_summaries;

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
}

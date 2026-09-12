//! HTTP handlers for Text Explorer: search, facets, and highlighted snippets.
//!
//! Auth posture: session-authenticated, read-only. Indexes are built on demand and
//! version-checked against the table, so a re-synced table is never served from a
//! stale index.

use axum::{
    extract::{Path, State},
    Json,
};

use crate::enrichment::display::DocDisplay;
use crate::shared::{derive_title, read_i64_at, read_id_at, read_string_at, AppResult};
use crate::state::AppState;
use crate::textexplore::highlight::{contains_term, highlight_runs, snippet_runs};
use crate::textexplore::index::{get_or_build, TextIndex};
use crate::textexplore::score::{common_terms, distinctive_terms, RankedTerm};
use crate::textexplore::types::{
    TextExploreRequest, TextExploreResponse, TextExploreRow, WordScore,
};

const MAX_LIMIT: usize = 500;

/// `POST /api/sources/{source_id}/tables/{table}/textexplore/search`
///
/// One round trip returns the filtered rows (with highlight runs) and the
/// common/distinctive words widget for the same subset.
pub async fn search(
    State(state): State<AppState>,
    Path((source_id, table)): Path<(String, String)>,
    Json(body): Json<TextExploreRequest>,
) -> AppResult<Json<TextExploreResponse>> {
    let limit = match body.limit {
        0 => crate::textexplore::types::default_limit(),
        n => n.min(MAX_LIMIT),
    };
    // Normalize once: matching is on lowercased text throughout.
    let includes: Vec<String> = normalized_terms(&body.terms, false);
    let excludes: Vec<String> = normalized_terms(&body.terms, true);
    let whole_word = body.whole_word;

    let index = get_or_build(&state, &source_id, &table).await?;
    let store = state.require_store()?;
    let columns: Vec<String> = index
        .df
        .get_column_names()
        .into_iter()
        .map(ToString::to_string)
        .collect();
    let display = DocDisplay::resolve(store, &source_id, &table, &columns).await?;

    let response = tokio::task::spawn_blocking(move || {
        build_response(&index, &display, &includes, &excludes, whole_word, limit)
    })
    .await?;
    Ok(Json(response))
}

fn normalized_terms(terms: &[crate::textexplore::types::SearchTerm], exclude: bool) -> Vec<String> {
    terms
        .iter()
        .filter(|t| t.exclude == exclude)
        .map(|t| t.text.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

fn build_response(
    index: &TextIndex,
    display: &DocDisplay,
    includes: &[String],
    excludes: &[String],
    whole_word: bool,
    limit: usize,
) -> TextExploreResponse {
    // 1. Filter: a row is kept iff every include term occurs and no exclude
    // term does.
    let matched: Vec<usize> = index
        .lowered
        .iter()
        .enumerate()
        .filter(|(_, text)| {
            includes.iter().all(|t| contains_term(text, t, whole_word))
                && !excludes.iter().any(|t| contains_term(text, t, whole_word))
        })
        .map(|(i, _)| i)
        .collect();

    // 2–4. Page the first `limit` matches in frame order and render them.
    let rows: Vec<TextExploreRow> = matched
        .iter()
        .take(limit)
        .map(|&i| render_row(index, display, i, includes, whole_word))
        .collect();

    // 5. Words widget over the whole matched subset.
    let has_filter = !includes.is_empty() || !excludes.is_empty();
    let subset_df: std::borrow::Cow<'_, [u32]> = if has_filter {
        std::borrow::Cow::Owned(subset_doc_freq(index, &matched))
    } else {
        // Unfiltered: the subset IS the corpus.
        std::borrow::Cow::Borrowed(index.corpus_df.as_slice())
    };
    // Terms already in the filter carry no information — zeroing their subset
    // count removes them from both lists (log-odds treats 0 as z = 0).
    let mut subset_df = subset_df.into_owned();
    for term in includes.iter().chain(excludes.iter()) {
        if let Some(id) = index.vocab.get_id(term) {
            subset_df[id as usize] = 0;
        }
    }

    let common = to_word_scores(index, &common_terms(&subset_df, matched.len()));
    // No filter ⇒ no subset to contrast ⇒ no distinctive list. A filter that
    // still matches everything degenerates to all-zero z-scores anyway.
    let distinctive = if has_filter {
        to_word_scores(index, &distinctive_terms(&subset_df, &index.corpus_df))
    } else {
        Vec::new()
    };

    TextExploreResponse {
        total_rows: index.corpus_rows,
        matched_rows: matched.len(),
        rows,
        common,
        distinctive,
        columns_searched: index.text_columns.clone(),
    }
}

/// Doc frequency per term over the matched rows, via a CSR walk.
fn subset_doc_freq(index: &TextIndex, matched: &[usize]) -> Vec<u32> {
    let mut counts = vec![0_u32; index.corpus_df.len()];
    for &row in matched {
        let (Some(&start), Some(&end)) = (
            index.row_terms_offsets.get(row),
            index.row_terms_offsets.get(row + 1),
        ) else {
            continue;
        };
        for &term_id in &index.row_terms_flat[start..end] {
            counts[term_id as usize] += 1;
        }
    }
    counts
}

fn to_word_scores(index: &TextIndex, ranked: &[RankedTerm]) -> Vec<WordScore> {
    ranked
        .iter()
        .filter_map(|r| {
            index.vocab.get_token(r.term_id).map(|term| WordScore {
                term: term.to_string(),
                count: r.count as usize,
                #[allow(clippy::cast_possible_truncation)]
                score: r.score as f32,
            })
        })
        .collect()
}

fn render_row(
    index: &TextIndex,
    display: &DocDisplay,
    row: usize,
    includes: &[String],
    whole_word: bool,
) -> TextExploreRow {
    let df = &index.df;
    let id = read_id_at(df, &display.id_column, row).unwrap_or_default();
    let number = display
        .number_column
        .as_deref()
        .and_then(|col| read_i64_at(df, col, row));
    let raw_body = display
        .body_column
        .as_deref()
        .and_then(|col| read_string_at(df, col, row));
    let title_text = match display.title_column.as_deref() {
        Some(col) => read_string_at(df, col, row),
        None => raw_body.as_deref().map(derive_title),
    }
    .unwrap_or_default();
    let html_url = display.url_at(df, row);
    let timestamp = display
        .timestamp_column
        .as_deref()
        .and_then(|col| read_string_at(df, col, row));

    TextExploreRow {
        id,
        number,
        title: highlight_runs(&title_text, includes, whole_word),
        snippet: snippet_runs(raw_body.as_deref().unwrap_or(""), includes, whole_word),
        html_url,
        timestamp,
    }
}

#[cfg(test)]
#[expect(
    clippy::shadow_unrelated,
    reason = "sequential test cases reuse binding names"
)]
mod tests {
    use super::*;
    use crate::textexplore::index::build_index;
    use polars::prelude::*;

    fn issues_index() -> TextIndex {
        let df = df!(
            "id" => &[1_i64, 2, 3, 4],
            "number" => &[11_i64, 12, 13, 14],
            "title" => &[
                "Parser panic on empty input",
                "Feature request: dark mode",
                "Parser panic when nested",
                "Docs typo in readme",
            ],
            "body" => &[
                Some("The parser panics with empty input files."),
                Some("Please add dark mode support."),
                None,
                Some("Small typo."),
            ],
            "html_url" => &["u1", "u2", "u3", "u4"],
            "created_at" => &["2026-01-01", "2026-01-02", "2026-01-03", "2026-01-04"],
        )
        .unwrap();
        build_index(df, 1, vec!["title".to_string(), "body".to_string()])
    }

    fn issues_display() -> DocDisplay {
        DocDisplay::infer(
            &["id", "number", "title", "body", "html_url", "created_at"].map(String::from),
        )
    }

    fn terms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn include_and_exclude_filtering() {
        let index = issues_index();
        let display = issues_display();
        let resp = build_response(&index, &display, &terms(&["panic"]), &[], false, 50);
        assert_eq!(resp.total_rows, 4);
        assert_eq!(resp.matched_rows, 2);
        assert_eq!(resp.rows.len(), 2);
        assert_eq!(resp.rows[0].id, "1");
        assert_eq!(resp.rows[0].number, Some(11));
        assert_eq!(resp.rows[0].html_url.as_deref(), Some("u1"));
        assert!(resp.rows[0].title.iter().any(|r| r.hl));

        let resp = build_response(
            &index,
            &display,
            &terms(&["panic"]),
            &terms(&["empty"]),
            false,
            50,
        );
        assert_eq!(resp.matched_rows, 1);
        assert_eq!(resp.rows[0].id, "3");
    }

    #[test]
    fn exclude_only_is_valid() {
        let index = issues_index();
        let display = issues_display();
        let resp = build_response(&index, &display, &[], &terms(&["panic"]), false, 50);
        assert_eq!(resp.matched_rows, 2);
    }

    #[test]
    fn empty_filter_returns_corpus_widget() {
        let index = issues_index();
        let display = issues_display();
        let resp = build_response(&index, &display, &[], &[], false, 50);
        assert_eq!(resp.matched_rows, 4);
        assert!(resp.distinctive.is_empty());
        assert!(!resp.common.is_empty());
        // No highlights without terms.
        assert!(resp.rows.iter().all(|r| r.title.iter().all(|run| !run.hl)));
        assert_eq!(resp.columns_searched, vec!["title", "body"]);
    }

    #[test]
    fn limit_pages_results() {
        let index = issues_index();
        let display = issues_display();
        let resp = build_response(&index, &display, &[], &[], false, 2);
        assert_eq!(resp.matched_rows, 4);
        assert_eq!(resp.rows.len(), 2);
    }

    #[test]
    fn filter_terms_dropped_from_widget() {
        let index = issues_index();
        let display = issues_display();
        let resp = build_response(&index, &display, &terms(&["parser"]), &[], false, 50);
        assert!(resp.common.iter().all(|w| w.term != "parser"));
        assert!(resp.distinctive.iter().all(|w| w.term != "parser"));
    }

    #[test]
    fn phrase_terms_match_across_words() {
        let index = issues_index();
        let display = issues_display();
        let resp = build_response(&index, &display, &terms(&["dark mode"]), &[], false, 50);
        assert_eq!(resp.matched_rows, 1);
        assert_eq!(resp.rows[0].id, "2");
    }

    #[test]
    fn whole_word_filter() {
        let index = issues_index();
        let display = issues_display();
        // "read" is a substring of "readme" only.
        let substring = build_response(&index, &display, &terms(&["read"]), &[], false, 50);
        assert_eq!(substring.matched_rows, 1);
        let whole = build_response(&index, &display, &terms(&["read"]), &[], true, 50);
        assert_eq!(whole.matched_rows, 0);
    }
}

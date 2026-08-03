//! Wire types for Text Explorer search, facets, and snippets.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One committed filter chip: an include or exclude word/phrase.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchTerm {
    /// Already unquoted; may contain spaces (phrase).
    pub text: String,
    #[serde(default)]
    pub exclude: bool,
}

/// Body for `POST …/textexplore/search`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TextExploreRequest {
    #[serde(default)]
    pub terms: Vec<SearchTerm>,
    /// Max rows to return (default 50, cap 500).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Match at word boundaries only instead of raw substrings.
    #[serde(default)]
    pub whole_word: bool,
}

pub(crate) fn default_limit() -> usize {
    50
}

/// One pre-segmented highlight run. Text crosses the wire pre-split so the
/// client never touches byte offsets (avoids Rust-byte vs JS-UTF-16 bugs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub t: String,
    pub hl: bool,
}

/// One matched row, rendered for the result list.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TextExploreRow {
    /// Row identifier, stringified (issue id, at:// uri, …).
    pub id: String,
    #[ts(optional)]
    pub number: Option<i64>,
    pub title: Vec<TextRun>,
    pub snippet: Vec<TextRun>,
    #[ts(optional)]
    pub html_url: Option<String>,
    #[ts(optional)]
    pub timestamp: Option<String>,
}

/// One term in the words widget.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct WordScore {
    pub term: String,
    /// Rows in the filtered subset containing this term.
    pub count: usize,
    /// Common list: share of subset rows. Distinctive list: log-odds z-score.
    pub score: f32,
}

/// Response for `POST …/textexplore/search`: rows + words widget together
/// (one filter pass, one loading state).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TextExploreResponse {
    pub total_rows: usize,
    pub matched_rows: usize,
    pub rows: Vec<TextExploreRow>,
    pub common: Vec<WordScore>,
    /// Empty when no include filter is active (no subset to contrast).
    pub distinctive: Vec<WordScore>,
    pub columns_searched: Vec<String>,
}

//! Highlight-run segmentation over case-folded matches.
//!
//! Matching happens on lowercased text (identical to the filter pass), but the
//! runs returned to the client carry the original text. `to_lowercase()` can
//! change byte lengths ('İ' → "i\u{307}", 'ß' → "ss"), so a per-byte offset
//! map translates match ranges in the lowered buffer back to valid
//! char-boundary ranges in the original.

use crate::textexplore::types::TextRun;

/// Lowered copy of a string plus, per lowered byte, the byte index of the
/// originating char in the original string.
pub(crate) struct LoweredText {
    pub lowered: String,
    map: Vec<usize>,
    original_len: usize,
}

impl LoweredText {
    pub fn new(original: &str) -> Self {
        let mut lowered = String::with_capacity(original.len());
        let mut map = Vec::with_capacity(original.len());
        for (orig_idx, ch) in original.char_indices() {
            let before = lowered.len();
            for lc in ch.to_lowercase() {
                lowered.push(lc);
            }
            for _ in before..lowered.len() {
                map.push(orig_idx);
            }
        }
        Self {
            lowered,
            map,
            original_len: original.len(),
        }
    }

    /// Translate a byte range in the lowered buffer to a char-boundary byte
    /// range in the original. Partial coverage of a multi-byte case expansion
    /// widens to include the whole originating char.
    fn to_original_range(&self, original: &str, start: usize, end: usize) -> (usize, usize) {
        let orig_start = self.map.get(start).copied().unwrap_or(self.original_len);
        let orig_end = match self.map.get(end) {
            None => self.original_len,
            Some(&idx) => {
                // `end` may land mid-expansion of a single original char
                // (matching only the first 's' of ß→"ss"): include that char.
                let prev = end.checked_sub(1).and_then(|i| self.map.get(i)).copied();
                if prev == Some(idx) {
                    idx + original[idx..].chars().next().map_or(0, char::len_utf8)
                } else {
                    idx
                }
            },
        };
        (orig_start, orig_end.max(orig_start))
    }
}

/// True when `hay[pos..pos+len]` sits at word boundaries (neighbors are not
/// alphanumeric).
fn at_word_boundary(hay: &str, pos: usize, end: usize) -> bool {
    let before_ok = pos == 0
        || !hay[..pos]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
    let after_ok =
        end >= hay.len() || !hay[end..].chars().next().is_some_and(char::is_alphanumeric);
    before_ok && after_ok
}

/// All occurrences of `needle` in `hay` (byte ranges), optionally
/// boundary-checked. Advances one char at a time so overlapping candidates
/// are all considered.
fn find_occurrences(hay: &str, needle: &str, whole_word: bool, out: &mut Vec<(usize, usize)>) {
    if needle.is_empty() {
        return;
    }
    let mut from = 0;
    while let Some(pos) = hay[from..].find(needle) {
        let abs = from + pos;
        let end = abs + needle.len();
        if !whole_word || at_word_boundary(hay, abs, end) {
            out.push((abs, end));
        }
        from = abs + hay[abs..].chars().next().map_or(1, char::len_utf8);
    }
}

/// True when `haystack_lowered` contains `needle` (already lowercased),
/// honoring whole-word mode. Shared by the row filter and the highlighter so
/// match and highlight semantics agree by construction.
pub(crate) fn contains_term(haystack_lowered: &str, needle: &str, whole_word: bool) -> bool {
    if !whole_word {
        return haystack_lowered.contains(needle);
    }
    let mut occurrences = Vec::new();
    find_occurrences(haystack_lowered, needle, true, &mut occurrences);
    !occurrences.is_empty()
}

/// Match ranges of the given lowercased terms in `original`, mapped back to
/// original byte offsets, merged where overlapping/adjacent, sorted.
fn match_ranges(
    original: &str,
    lowered: &LoweredText,
    terms: &[String],
    whole_word: bool,
) -> Vec<(usize, usize)> {
    let mut raw = Vec::new();
    let mut in_lowered = Vec::new();
    for term in terms {
        in_lowered.clear();
        find_occurrences(&lowered.lowered, term, whole_word, &mut in_lowered);
        for &(s, e) in &in_lowered {
            let range = lowered.to_original_range(original, s, e);
            if range.1 > range.0 {
                raw.push(range);
            }
        }
    }
    raw.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(raw.len());
    for (s, e) in raw {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
}

/// Segment `original` into alternating runs, highlighting every occurrence of
/// the (already lowercased) terms.
pub(crate) fn highlight_runs(original: &str, terms: &[String], whole_word: bool) -> Vec<TextRun> {
    if original.is_empty() {
        return Vec::new();
    }
    if terms.is_empty() {
        return vec![TextRun {
            t: original.to_string(),
            hl: false,
        }];
    }
    let lowered = LoweredText::new(original);
    let ranges = match_ranges(original, &lowered, terms, whole_word);
    runs_from_ranges(original, &ranges)
}

fn runs_from_ranges(original: &str, ranges: &[(usize, usize)]) -> Vec<TextRun> {
    let mut runs = Vec::with_capacity(ranges.len() * 2 + 1);
    let mut cursor = 0;
    for &(s, e) in ranges {
        if s > cursor {
            runs.push(TextRun {
                t: original[cursor..s].to_string(),
                hl: false,
            });
        }
        runs.push(TextRun {
            t: original[s..e].to_string(),
            hl: true,
        });
        cursor = e;
    }
    if cursor < original.len() {
        runs.push(TextRun {
            t: original[cursor..].to_string(),
            hl: false,
        });
    }
    if runs.is_empty() {
        runs.push(TextRun {
            t: original.to_string(),
            hl: false,
        });
    }
    runs
}

/// Chars of context kept on each side of the first body match.
const SNIPPET_CONTEXT_CHARS: usize = 120;
/// Body-head fallback length when no term matches the body.
const SNIPPET_HEAD_CHARS: usize = 240;

/// Ellipsis run used to mark a truncated snippet edge.
fn ellipsis() -> TextRun {
    TextRun {
        t: "…".to_string(),
        hl: false,
    }
}

/// Char-boundary byte index `n` chars before `pos` in `s`.
fn walk_back(s: &str, pos: usize, n: usize) -> usize {
    s[..pos]
        .char_indices()
        .rev()
        .take(n)
        .last()
        .map_or(pos, |(i, _)| i)
}

/// Char-boundary byte index `n` chars after `pos` in `s`.
fn walk_forward(s: &str, pos: usize, n: usize) -> usize {
    s[pos..]
        .char_indices()
        .nth(n)
        .map_or(s.len(), |(i, _)| pos + i)
}

/// Snippet for a body: a window around the first match with highlights, or
/// the body head when nothing matches.
pub(crate) fn snippet_runs(body: &str, terms: &[String], whole_word: bool) -> Vec<TextRun> {
    if body.is_empty() {
        return Vec::new();
    }
    let first_match = if terms.is_empty() {
        None
    } else {
        let lowered = LoweredText::new(body);
        match_ranges(body, &lowered, terms, whole_word)
            .first()
            .copied()
    };

    let Some((match_start, match_end)) = first_match else {
        let head_end = walk_forward(body, 0, SNIPPET_HEAD_CHARS);
        let mut runs = vec![TextRun {
            t: body[..head_end].to_string(),
            hl: false,
        }];
        if head_end < body.len() {
            runs.push(ellipsis());
        }
        return runs;
    };

    let window_start = walk_back(body, match_start, SNIPPET_CONTEXT_CHARS);
    let window_end = walk_forward(body, match_end, SNIPPET_CONTEXT_CHARS);
    let mut runs = Vec::new();
    if window_start > 0 {
        runs.push(ellipsis());
    }
    runs.extend(highlight_runs(
        &body[window_start..window_end],
        terms,
        whole_word,
    ));
    if window_end < body.len() {
        runs.push(ellipsis());
    }
    runs
}

#[cfg(test)]
#[allow(clippy::shadow_unrelated)]
mod tests {
    use super::*;

    fn terms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    fn joined(runs: &[TextRun]) -> String {
        runs.iter().map(|r| r.t.as_str()).collect()
    }

    #[test]
    fn basic_highlight() {
        let runs = highlight_runs("Panic in parser", &terms(&["panic"]), false);
        assert_eq!(
            runs,
            vec![
                TextRun {
                    t: "Panic".to_string(),
                    hl: true
                },
                TextRun {
                    t: " in parser".to_string(),
                    hl: false
                },
            ]
        );
    }

    #[test]
    fn runs_reassemble_original() {
        let text = "The parser panics when the parser sees EOF";
        let runs = highlight_runs(text, &terms(&["parser", "eof"]), false);
        assert_eq!(joined(&runs), text);
        assert_eq!(runs.iter().filter(|r| r.hl).count(), 3);
    }

    #[test]
    fn overlapping_terms_merge() {
        let runs = highlight_runs("deadlock", &terms(&["dead", "adlock"]), false);
        assert_eq!(
            runs,
            vec![TextRun {
                t: "deadlock".to_string(),
                hl: true
            }]
        );
    }

    #[test]
    fn no_terms_single_run() {
        let runs = highlight_runs("hello", &[], false);
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].hl);
    }

    #[test]
    fn turkish_dotted_capital_i() {
        // 'İ' (2 bytes) lowercases to "i\u{307}" (3 bytes): every lowered
        // byte must map back to a valid char boundary in the original.
        let text = "İstanbul crash";
        let runs = highlight_runs(text, &terms(&["crash"]), false);
        assert_eq!(joined(&runs), text);
        assert!(runs.iter().any(|r| r.hl && r.t == "crash"));
        // A term matching into the expansion still yields valid runs.
        let runs = highlight_runs(text, &terms(&["i\u{307}stanbul"]), false);
        assert_eq!(joined(&runs), text);
        assert!(runs.iter().any(|r| r.hl && r.t == "İstanbul"));
    }

    #[test]
    fn sharp_s_stays_itself() {
        // 'ß' is already lowercase ("Straße".to_lowercase() == "straße"), so
        // "strasse" matches only STRASSE — identical to the filter's
        // semantics, since both sides lower with the same to_lowercase().
        let text = "STRASSE and Straße";
        let runs = highlight_runs(text, &terms(&["strasse"]), false);
        assert_eq!(joined(&runs), text);
        let hits: Vec<&str> = runs.iter().filter(|r| r.hl).map(|r| r.t.as_str()).collect();
        assert_eq!(hits, vec!["STRASSE"]);
        let runs = highlight_runs(text, &terms(&["straße"]), false);
        let hits: Vec<&str> = runs.iter().filter(|r| r.hl).map(|r| r.t.as_str()).collect();
        assert_eq!(hits, vec!["Straße"]);
    }

    #[test]
    fn partial_expansion_widens_to_full_char() {
        // 'İ' lowers to "i\u{307}" (1 char → 2 chars, 2 bytes → 3 bytes).
        // A term consuming only the leading "i" of that expansion must widen
        // to cover the whole 'İ' — never emit a split char.
        let runs = highlight_runs("İX", &terms(&["i"]), false);
        assert_eq!(joined(&runs), "İX");
        assert_eq!(runs[0].t, "İ");
        assert!(runs[0].hl);
    }

    #[test]
    fn emoji_boundaries() {
        let text = "fix 🐛 in tokenizer 🎉";
        let runs = highlight_runs(text, &terms(&["tokenizer"]), false);
        assert_eq!(joined(&runs), text);
        assert!(runs.iter().any(|r| r.hl && r.t == "tokenizer"));
    }

    #[test]
    fn whole_word_mode() {
        assert!(contains_term("the cat sat", "cat", true));
        assert!(!contains_term("concatenate", "cat", true));
        assert!(contains_term("cat.", "cat", true));
        assert!(contains_term("(cat)", "cat", true));
        let runs = highlight_runs("cat concatenate cat", &terms(&["cat"]), true);
        assert_eq!(runs.iter().filter(|r| r.hl).count(), 2);
    }

    #[test]
    fn overlapping_occurrences_found() {
        // "aa" in "aaa" at whole-word off: positions 0 and 1 merge into one.
        let runs = highlight_runs("aaa", &terms(&["aa"]), false);
        assert_eq!(
            runs,
            vec![TextRun {
                t: "aaa".to_string(),
                hl: true
            }]
        );
    }

    #[test]
    fn snippet_windows_around_first_match() {
        let body = format!("{}panic here{}", "x".repeat(300), "y".repeat(300));
        let runs = snippet_runs(&body, &terms(&["panic"]), false);
        assert_eq!(runs.first().unwrap().t, "…");
        assert_eq!(runs.last().unwrap().t, "…");
        assert!(runs.iter().any(|r| r.hl && r.t == "panic"));
        // Window: ~120 chars each side plus the match and ellipses.
        let total: usize = runs.iter().map(|r| r.t.chars().count()).sum();
        assert!(total < 260);
    }

    #[test]
    fn snippet_falls_back_to_head() {
        let body = "z".repeat(500);
        let runs = snippet_runs(&body, &terms(&["missing"]), false);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].t.chars().count(), 240);
        assert_eq!(runs[1].t, "…");
    }

    #[test]
    fn snippet_short_body_no_ellipsis() {
        let runs = snippet_runs("short body", &[], false);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].t, "short body");
    }
}

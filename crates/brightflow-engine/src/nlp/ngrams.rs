//! Word n-gram generation over token slices.
//!
//! Unigrams are yielded borrowed and only multi-word grams allocate, which
//! matters because this runs over every document in a corpus and unigrams are
//! the overwhelming majority of what it produces.

use std::borrow::Cow;
use std::ops::RangeInclusive;

/// Generate word n-grams over a token slice.
///
/// For `range = 1..=2` and tokens `["the", "quick", "fox"]`, yields:
/// `"the"`, `"quick"`, `"fox"`, `"the quick"`, `"quick fox"`.
///
/// Unigrams are yielded as `Cow::Borrowed` (no allocation).
/// Multi-word n-grams are joined with spaces and yielded as `Cow::Owned`.
pub fn ngrams(
    tokens: &[String],
    range: RangeInclusive<usize>,
) -> impl Iterator<Item = Cow<'_, str>> {
    let start = *range.start();
    let end = *range.end();

    (start..=end).flat_map(move |n| {
        if n == 0 || n > tokens.len() {
            return Vec::new().into_iter();
        }
        if n == 1 {
            tokens
                .iter()
                .map(|t| Cow::Borrowed(t.as_str()))
                .collect::<Vec<_>>()
                .into_iter()
        } else {
            tokens
                .windows(n)
                .map(|window| {
                    let joined: String = window.join(" ");
                    Cow::Owned(joined)
                })
                .collect::<Vec<_>>()
                .into_iter()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn test_unigrams() {
        let tokens = strs(&["hello", "world", "rust"]);
        let result: Vec<String> = ngrams(&tokens, 1..=1).map(Cow::into_owned).collect();
        assert_eq!(result, vec!["hello", "world", "rust"]);
    }

    #[test]
    fn test_bigrams() {
        let tokens = strs(&["hello", "world", "rust"]);
        let result: Vec<String> = ngrams(&tokens, 2..=2).map(Cow::into_owned).collect();
        assert_eq!(result, vec!["hello world", "world rust"]);
    }

    #[test]
    fn test_mixed_range() {
        let tokens = strs(&["the", "quick", "fox"]);
        let result: Vec<String> = ngrams(&tokens, 1..=2).map(Cow::into_owned).collect();
        assert_eq!(
            result,
            vec!["the", "quick", "fox", "the quick", "quick fox"]
        );
    }

    #[test]
    fn test_trigrams() {
        let tokens = strs(&["a", "b", "c", "d"]);
        let result: Vec<String> = ngrams(&tokens, 1..=3).map(Cow::into_owned).collect();
        assert_eq!(
            result,
            vec!["a", "b", "c", "d", "a b", "b c", "c d", "a b c", "b c d"]
        );
    }

    #[test]
    fn test_empty_tokens() {
        let tokens: Vec<String> = Vec::new();
        assert_eq!(ngrams(&tokens, 1..=2).count(), 0);
    }

    #[test]
    fn test_single_token() {
        let tokens = strs(&["hello"]);
        let result: Vec<String> = ngrams(&tokens, 1..=2).map(Cow::into_owned).collect();
        // Only unigram, bigram requires 2 tokens
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn test_cow_borrowing() {
        let tokens = strs(&["hello", "world"]);
        let grams: Vec<Cow<'_, str>> = ngrams(&tokens, 1..=1).collect();
        // Unigrams should be borrowed
        for g in &grams {
            assert!(matches!(g, Cow::Borrowed(_)));
        }
    }
}

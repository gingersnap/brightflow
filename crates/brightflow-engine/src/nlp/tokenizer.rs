//! Text to tokens, with byte-offset spans preserved.
//!
//! `tokenize` yields byte-offset `TokenSpan`s rather than owned strings, so a
//! caller can map any token back to its position in the source text — needed by
//! anything that highlights or excerpts the original rather than the tokens.
//! `token_text` does the lookup when only the text is wanted.
//!
//! Unicode segmentation rather than whitespace splitting, so non-Latin scripts
//! tokenize sanely.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

/// A byte-offset span into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenSpan {
    pub start: usize,
    pub end: usize,
}

/// A configurable word tokenizer.
///
/// Two presets are provided: `code_aware()` for code-mixed text (GitHub issues,
/// logs, support tickets) and `plain()` for natural language prose.
/// Use `builder()` for full control.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    lowercase: bool,
    min_token_len: usize,
    max_token_len: usize,
    split_camel_case: bool,
    preserve_code_tokens: bool,
    stop_words: HashSet<String>,
}

/// Builder for configuring a [`Tokenizer`].
#[derive(Debug, Clone)]
pub struct TokenizerBuilder {
    inner: Tokenizer,
}

// Regex for detecting code-like tokens that should be preserved as-is:
// file paths, namespaces, error codes, version strings.
#[allow(clippy::unwrap_used)]
static CODE_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        [\w./\\-]+\.[\w]{1,5}     # file paths: src/main.rs, foo/bar.txt
        | [\w]+::[\w:]+            # namespace paths: tokio::spawn, std::io::Error
        | [Ee]\d{3,5}             # error codes: E0308, e0001
        | [Vv]\d+(?:\.\d+)*      # version strings: v1.2.3, V2
        | [A-Z][A-Z0-9_]{3,}     # constant-style: ECONNREFUSED, NULL_PTR
    ",
    )
    .unwrap()
});

#[allow(clippy::unwrap_used)]
static CAMEL_BOUNDARY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z])([A-Z])|([A-Z]+)([A-Z][a-z])").unwrap());

impl Tokenizer {
    /// Preset for code-mixed text: CamelCase splitting, code pattern recognition,
    /// lowercasing, min token length 2.
    pub fn code_aware() -> Self {
        Self {
            lowercase: true,
            min_token_len: 2,
            max_token_len: 100,
            split_camel_case: true,
            preserve_code_tokens: true,
            stop_words: HashSet::new(),
        }
    }

    /// Preset for natural language prose: simple word splitting, lowercasing,
    /// min token length 2.
    pub fn plain() -> Self {
        Self {
            lowercase: true,
            min_token_len: 2,
            max_token_len: 100,
            split_camel_case: false,
            preserve_code_tokens: false,
            stop_words: HashSet::new(),
        }
    }

    /// Full control via builder pattern.
    pub fn builder() -> TokenizerBuilder {
        TokenizerBuilder {
            inner: Self::plain(),
        }
    }

    /// Tokenize text into byte-offset spans.
    pub fn tokenize(&self, text: &str) -> Vec<TokenSpan> {
        let mut spans = Vec::new();

        if self.preserve_code_tokens {
            self.tokenize_code_aware(text, &mut spans);
        } else {
            self.tokenize_plain(text, &mut spans);
        }

        spans
    }

    /// Extract the text for a token span, applying lowercasing if configured.
    pub fn token_text<'a>(&self, text: &'a str, span: &TokenSpan) -> Cow<'a, str> {
        let slice = &text[span.start..span.end];
        if self.lowercase {
            let lower = slice.to_lowercase();
            if lower == slice {
                Cow::Borrowed(slice)
            } else {
                Cow::Owned(lower)
            }
        } else {
            Cow::Borrowed(slice)
        }
    }

    /// Convenience: tokenize and collect as owned strings.
    pub fn tokenize_to_strings(&self, text: &str) -> Vec<String> {
        let spans = self.tokenize(text);
        spans
            .iter()
            .flat_map(|span| {
                let original = &text[span.start..span.end];
                let token = self.token_text(text, span);
                if self.split_camel_case && has_camel_case(original) {
                    // Split on original case, then lowercase the parts
                    let parts = split_camel_case_word(original);
                    if parts.len() > 1 {
                        // Emit both compound and parts
                        let mut result = vec![token.into_owned()];
                        result.extend(parts);
                        return result;
                    }
                }
                vec![token.into_owned()]
            })
            .filter(|t| {
                t.len() >= self.min_token_len
                    && t.len() <= self.max_token_len
                    && !self.stop_words.contains(t)
            })
            .collect()
    }

    fn tokenize_plain(&self, text: &str, spans: &mut Vec<TokenSpan>) {
        for (offset, word) in text.split_word_bound_indices() {
            if !word.chars().any(char::is_alphanumeric) {
                continue;
            }
            let len = word.len();
            if len < self.min_token_len || len > self.max_token_len {
                continue;
            }
            let token_lower = if self.lowercase {
                word.to_lowercase()
            } else {
                word.to_string()
            };
            if self.stop_words.contains(&token_lower) {
                continue;
            }
            spans.push(TokenSpan {
                start: offset,
                end: offset + len,
            });
        }
    }

    fn tokenize_code_aware(&self, text: &str, spans: &mut Vec<TokenSpan>) {
        // First pass: find code-token regions that should be preserved whole
        let mut code_regions: Vec<(usize, usize)> = Vec::new();
        for mat in CODE_TOKEN_RE.find_iter(text) {
            code_regions.push((mat.start(), mat.end()));
        }

        // Process text, yielding code tokens whole and splitting the rest normally
        let mut pos = 0;
        let mut region_idx = 0;

        while pos < text.len() {
            // Check if current position is at a code token
            if region_idx < code_regions.len() && pos <= code_regions[region_idx].0 {
                // Process plain text before the code token
                if pos < code_regions[region_idx].0 {
                    let segment = &text[pos..code_regions[region_idx].0];
                    self.tokenize_segment(segment, pos, spans);
                }
                // Emit the code token as a single span
                let (start, end) = code_regions[region_idx];
                let word = &text[start..end];
                if word.len() >= self.min_token_len && word.len() <= self.max_token_len {
                    let token_lower = if self.lowercase {
                        word.to_lowercase()
                    } else {
                        word.to_string()
                    };
                    if !self.stop_words.contains(&token_lower) {
                        spans.push(TokenSpan { start, end });
                    }
                }
                pos = end;
                region_idx += 1;
            } else {
                // No more code regions, process the rest normally
                let segment = &text[pos..];
                self.tokenize_segment(segment, pos, spans);
                pos = text.len();
            }
        }
    }

    fn tokenize_segment(&self, segment: &str, base_offset: usize, spans: &mut Vec<TokenSpan>) {
        for (offset, word) in segment.split_word_bound_indices() {
            if !word.chars().any(char::is_alphanumeric) {
                continue;
            }
            let len = word.len();
            if len < self.min_token_len || len > self.max_token_len {
                continue;
            }
            let token_lower = if self.lowercase {
                word.to_lowercase()
            } else {
                word.to_string()
            };
            if self.stop_words.contains(&token_lower) {
                continue;
            }
            spans.push(TokenSpan {
                start: base_offset + offset,
                end: base_offset + offset + len,
            });
        }
    }
}

impl TokenizerBuilder {
    /// Set whether to lowercase tokens (default: true).
    pub fn lowercase(mut self, lowercase: bool) -> Self {
        self.inner.lowercase = lowercase;
        self
    }

    /// Set minimum token length (default: 2).
    pub fn min_token_len(mut self, len: usize) -> Self {
        self.inner.min_token_len = len;
        self
    }

    /// Set maximum token length (default: 100).
    pub fn max_token_len(mut self, len: usize) -> Self {
        self.inner.max_token_len = len;
        self
    }

    /// Enable CamelCase splitting (default: false for plain, true for code_aware).
    pub fn split_camel_case(mut self, split: bool) -> Self {
        self.inner.split_camel_case = split;
        self
    }

    /// Enable code-pattern preservation (default: false for plain, true for code_aware).
    pub fn preserve_code_tokens(mut self, preserve: bool) -> Self {
        self.inner.preserve_code_tokens = preserve;
        self
    }

    /// Set stop words to filter out.
    pub fn stop_words(mut self, words: HashSet<String>) -> Self {
        self.inner.stop_words = words;
        self
    }

    /// Build the tokenizer.
    pub fn build(self) -> Tokenizer {
        self.inner
    }
}

/// Check if a word contains CamelCase boundaries.
fn has_camel_case(word: &str) -> bool {
    let mut has_lower = false;
    let mut has_upper_after_lower = false;
    for ch in word.chars() {
        if ch.is_lowercase() {
            has_lower = true;
        } else if ch.is_uppercase() && has_lower {
            has_upper_after_lower = true;
            break;
        }
    }
    has_upper_after_lower
}

/// Split a CamelCase word into its parts (lowercased).
fn split_camel_case_word(word: &str) -> Vec<String> {
    let spaced = CAMEL_BOUNDARY_RE.replace_all(word, "$1$3 $2$4");
    let parts: Vec<String> = spaced
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|s| s.len() >= 2)
        .collect();
    if parts.len() <= 1 {
        return Vec::new();
    }
    parts
}

/// Preset identifier for serialization (used by `FittedTfIdf` to reconstruct tokenizer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TokenizerPreset {
    CodeAware,
    Plain,
}

impl TokenizerPreset {
    /// Reconstruct a tokenizer from a preset.
    pub fn build(self) -> Tokenizer {
        match self {
            Self::CodeAware => Tokenizer::code_aware(),
            Self::Plain => Tokenizer::plain(),
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::shadow_unrelated,
    reason = "sequential test cases reuse binding names"
)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_basic() {
        let t = Tokenizer::plain();
        let tokens = t.tokenize_to_strings("hello world");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_plain_punctuation() {
        let t = Tokenizer::plain();
        let tokens = t.tokenize_to_strings("hello, world! how are you?");
        assert_eq!(tokens, vec!["hello", "world", "how", "are", "you"]);
    }

    #[test]
    fn test_plain_uppercase() {
        let t = Tokenizer::plain();
        let tokens = t.tokenize_to_strings("Hello World");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_plain_min_length() {
        let t = Tokenizer::plain();
        let tokens = t.tokenize_to_strings("I am a big cat");
        // "I", "a" are length 1, filtered out
        assert_eq!(tokens, vec!["am", "big", "cat"]);
    }

    #[test]
    fn test_code_aware_camel_case() {
        let t = Tokenizer::code_aware();
        let tokens = t.tokenize_to_strings("NullPointerException occurred");
        // Should emit: "nullpointerexception", "null", "pointer", "exception", "occurred"
        assert!(tokens.contains(&"nullpointerexception".to_string()));
        assert!(tokens.contains(&"null".to_string()));
        assert!(tokens.contains(&"pointer".to_string()));
        assert!(tokens.contains(&"exception".to_string()));
        assert!(tokens.contains(&"occurred".to_string()));
    }

    #[test]
    fn test_code_aware_error_code() {
        let t = Tokenizer::code_aware();
        let tokens = t.tokenize_to_strings("got error E0308 in compile");
        assert!(tokens.iter().any(|t| t == "e0308"));
    }

    #[test]
    fn test_code_aware_version_string() {
        let t = Tokenizer::code_aware();
        let tokens = t.tokenize_to_strings("upgraded to v1.2.3 today");
        assert!(tokens.iter().any(|t| t == "v1.2.3"));
    }

    #[test]
    fn test_code_aware_namespace() {
        let t = Tokenizer::code_aware();
        let tokens = t.tokenize_to_strings("called tokio::spawn and it panicked");
        assert!(tokens.iter().any(|t| t == "tokio::spawn"));
    }

    #[test]
    fn test_stop_words() {
        let stop: HashSet<String> = ["the", "and", "is"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let t = Tokenizer::builder().stop_words(stop).build();
        let tokens = t.tokenize_to_strings("the cat is big and fluffy");
        assert_eq!(tokens, vec!["cat", "big", "fluffy"]);
    }

    #[test]
    fn test_empty_input() {
        let t = Tokenizer::plain();
        let tokens = t.tokenize_to_strings("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_no_lowercase() {
        let t = Tokenizer::builder().lowercase(false).build();
        let tokens = t.tokenize_to_strings("Hello World");
        assert_eq!(tokens, vec!["Hello", "World"]);
    }

    #[test]
    fn test_token_spans() {
        let t = Tokenizer::plain();
        let text = "hello world";
        let spans = t.tokenize(text);
        assert_eq!(spans.len(), 2);
        assert_eq!(&text[spans[0].start..spans[0].end], "hello");
        assert_eq!(&text[spans[1].start..spans[1].end], "world");
    }

    #[test]
    fn test_unicode_text() {
        let t = Tokenizer::plain();
        let tokens = t.tokenize_to_strings("caf\u{e9} na\u{ef}ve r\u{e9}sum\u{e9}");
        assert_eq!(tokens, vec!["caf\u{e9}", "na\u{ef}ve", "r\u{e9}sum\u{e9}"]);
    }

    #[test]
    fn test_preset_roundtrip() {
        let preset = TokenizerPreset::CodeAware;
        let t = preset.build();
        let tokens = t.tokenize_to_strings("hello world");
        assert_eq!(tokens, vec!["hello", "world"]);
    }
}

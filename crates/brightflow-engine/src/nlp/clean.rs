//! Text cleaning applied BEFORE embedding.
//!
//! Static embedders like Model2Vec average token vectors, so URLs, @handles,
//! markdown syntax, and emoji dominate the embedding of short texts and the
//! clusterer ends up grouping documents by *format* instead of content
//! ("posts with links", "issues with code blocks"). Cleaning is therefore
//! part of the embedding contract: the cleaning profile + version are baked
//! into the effective model id, so changing a cleaner automatically
//! invalidates cached embeddings.

use regex::Regex;
use std::sync::LazyLock;

/// Bump when any cleaner's behavior changes — flows into profile ids and
/// therefore into effective model ids, forcing re-embedding.
pub const CLEAN_VERSION: u32 = 1;

/// Minimum whitespace-delimited tokens a cleaned text needs to be worth
/// embedding. Rows below the gate get no embedding and no cluster.
pub const MIN_EMBED_TOKENS: usize = 3;

/// Minimum CJK characters that also qualify a text (CJK scripts don't use
/// whitespace tokenization, so the token gate alone would drop them).
const MIN_CJK_CHARS: usize = 6;

/// Versioned cleaning profile. The id (e.g. `social.v1`) is embedded in the
/// effective model id — see [`effective_model_id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleaningProfile {
    /// Short social posts: URLs, @handles, #hashtags, emoji.
    Social,
    /// GitHub-style issues: fenced code, markdown syntax, template boilerplate.
    MarkdownIssue,
    /// Whitespace normalization only.
    Plain,
}

impl CleaningProfile {
    /// Stable versioned identifier, part of the effective model id.
    pub fn id(self) -> String {
        let name = match self {
            Self::Social => "social",
            Self::MarkdownIssue => "markdown_issue",
            Self::Plain => "plain",
        };
        format!("{name}.v{CLEAN_VERSION}")
    }

    /// Parse a profile name (without version) as stored in settings.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "social" => Some(Self::Social),
            "markdown_issue" => Some(Self::MarkdownIssue),
            "plain" => Some(Self::Plain),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Social => "social",
            Self::MarkdownIssue => "markdown_issue",
            Self::Plain => "plain",
        }
    }
}

/// Embedding model id that includes the cleaning contract, e.g.
/// `potion-base-32M#clean=social.v1`. Cached row embeddings are keyed by this,
/// so cleaning changes lazily re-embed with zero migration.
pub fn effective_model_id(base_model_id: &str, profile: CleaningProfile) -> String {
    format!("{base_model_id}#clean={}", profile.id())
}

/// Clean a text for embedding; `None` when it is too thin to embed.
///
/// "Too thin" means fewer than [`MIN_EMBED_TOKENS`] tokens and not CJK.
/// Callers must leave such rows unembedded and unclustered rather than
/// feeding zero vectors to the clusterer.
pub fn clean_for_embedding(text: &str, profile: CleaningProfile) -> Option<String> {
    let cleaned = clean(text, profile);
    if is_embeddable(&cleaned) {
        Some(cleaned)
    } else {
        None
    }
}

/// Apply a profile's cleaner chain without the eligibility gate.
pub fn clean(text: &str, profile: CleaningProfile) -> String {
    match profile {
        CleaningProfile::Social => {
            let t = strip_urls(text);
            let t = strip_mentions(&t);
            let t = unhash_hashtags(&t);
            let t = strip_emoji(&t);
            collapse_whitespace(&t)
        },
        CleaningProfile::MarkdownIssue => {
            let t = strip_code_fences(text);
            let t = strip_issue_boilerplate(&t);
            let t = strip_urls(&t);
            let t = strip_markdown(&t);
            let t = strip_emoji(&t);
            collapse_whitespace(&t)
        },
        CleaningProfile::Plain => collapse_whitespace(text),
    }
}

fn is_embeddable(cleaned: &str) -> bool {
    if cleaned.split_whitespace().count() >= MIN_EMBED_TOKENS {
        return true;
    }
    cleaned.chars().filter(|&c| is_cjk(c)).count() >= MIN_CJK_CHARS
}

// ─── Individual cleaners ──────────────────────────────────────────────────────
// Statics use expect() on literal regexes — a failure is a programmer error
// caught by the unit tests, matching the tokenizer.rs precedent.

#[allow(clippy::expect_used)]
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:https?://|www\.)\S+").expect("valid regex"));

/// `@handle` and dotted handles like `@user.bsky.social`.
#[allow(clippy::expect_used)]
static MENTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@[A-Za-z0-9_](?:[A-Za-z0-9_.-]*[A-Za-z0-9_])?").expect("valid regex")
});

#[allow(clippy::expect_used)]
static HASHTAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(\w)").expect("valid regex"));

#[allow(clippy::expect_used)]
static CODE_FENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```.*?(?:```|\z)|~~~.*?(?:~~~|\z)").expect("valid regex"));

#[allow(clippy::expect_used)]
static HTML_COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<!--.*?(?:-->|\z)").expect("valid regex"));

#[allow(clippy::expect_used)]
static HTML_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"</?[A-Za-z][^>\n]*>").expect("valid regex"));

#[allow(clippy::expect_used)]
static MD_IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\([^)]*\)").expect("valid regex"));

#[allow(clippy::expect_used)]
static MD_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").expect("valid regex"));

#[allow(clippy::expect_used)]
static MD_HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s{0,3}#{1,6}\s+").expect("valid regex"));

#[allow(clippy::expect_used)]
static MD_EMPHASIS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[*_~`]{1,3}").expect("valid regex"));

#[allow(clippy::expect_used)]
static MD_QUOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s{0,3}>\s?").expect("valid regex"));

#[allow(clippy::expect_used)]
static CHECKBOX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*[-*+]\s+\[[ xX]\]\s*").expect("valid regex"));

/// Common issue-template headings that carry zero topical signal.
#[allow(clippy::expect_used)]
static BOILERPLATE_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?im)^\s{0,3}(?:\*\*|#{1,6}\s*)?(?:describe the bug|bug description|expected behaviou?r|actual behaviou?r|steps? to reproduce|to reproduce|how to reproduce|reproduction(?: steps)?|screenshots?|additional context|system info(?:rmation)?|environment|your environment|version(?:s)? affected|possible solution|what happened\??|what did you expect(?: to happen)?\??|minimal reproducible example)(?:\*\*|:)?\s*$",
    )
    .expect("valid regex")
});

#[allow(clippy::expect_used)]
static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));

pub fn strip_urls(text: &str) -> String {
    URL_RE.replace_all(text, " ").into_owned()
}

pub fn strip_mentions(text: &str) -> String {
    MENTION_RE.replace_all(text, " ").into_owned()
}

/// `#rustlang` → `rustlang` (keeps the topical word, drops the syntax).
pub fn unhash_hashtags(text: &str) -> String {
    HASHTAG_RE.replace_all(text, "$1").into_owned()
}

/// Remove emoji and pictographs by codepoint range. Deliberately narrow:
/// must NOT touch CJK ideographs, kana, hangul, or CJK punctuation.
pub fn strip_emoji(text: &str) -> String {
    text.chars().filter(|&c| !is_emoji(c)).collect()
}

fn is_emoji(c: char) -> bool {
    matches!(u32::from(c),
        0x1F000..=0x1F0FF   // mahjong, dominoes, playing cards
        | 0x1F100..=0x1F1FF // enclosed alphanumeric supplement + regional indicators
        | 0x1F300..=0x1FAFF // pictographs, emoticons, transport, supplemental, extended-A
        | 0x2600..=0x27BF   // misc symbols + dingbats
        | 0x2B00..=0x2BFF   // misc symbols and arrows (⭐ etc.)
        | 0xFE00..=0xFE0F   // variation selectors
        | 0x200D            // zero-width joiner
        | 0x20E3            // combining enclosing keycap
    )
}

fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x3040..=0x30FF     // hiragana + katakana
        | 0x3400..=0x4DBF   // CJK extension A
        | 0x4E00..=0x9FFF   // CJK unified ideographs
        | 0xAC00..=0xD7AF   // hangul syllables
        | 0xF900..=0xFAFF   // CJK compatibility ideographs
    )
}

/// Remove fenced code blocks entirely (their tokens are format, not topic).
pub fn strip_code_fences(text: &str) -> String {
    CODE_FENCE_RE.replace_all(text, " ").into_owned()
}

/// Remove markdown syntax while keeping the human-readable content:
/// images/links keep their alt/label text, emphasis and heading markers drop.
pub fn strip_markdown(text: &str) -> String {
    let t = HTML_COMMENT_RE.replace_all(text, " ");
    let t = HTML_TAG_RE.replace_all(&t, " ");
    let t = MD_IMAGE_RE.replace_all(&t, "$1");
    let t = MD_LINK_RE.replace_all(&t, "$1");
    let t = MD_HEADING_RE.replace_all(&t, "");
    let t = MD_QUOTE_RE.replace_all(&t, "");
    let t = CHECKBOX_RE.replace_all(&t, "");
    MD_EMPHASIS_RE.replace_all(&t, "").into_owned()
}

/// Remove issue-template scaffolding: HTML comments (template instructions)
/// and stock section headings like "Steps to reproduce".
pub fn strip_issue_boilerplate(text: &str) -> String {
    let t = HTML_COMMENT_RE.replace_all(text, " ");
    BOILERPLATE_LINE_RE.replace_all(&t, " ").into_owned()
}

pub fn collapse_whitespace(text: &str) -> String {
    WHITESPACE_RE
        .replace_all(text.trim(), " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn social_profile_cleans_bluesky_post() {
        let post = "Check this out 🚀🔥 https://example.com/article?utm=x \
                    @friend.bsky.social #rustlang is great 日本語のテキストも大丈夫 👍";
        let cleaned = clean_for_embedding(post, CleaningProfile::Social).unwrap();
        assert!(!cleaned.contains("https://"), "{cleaned}");
        assert!(!cleaned.contains("example.com"), "{cleaned}");
        assert!(!cleaned.contains('@'), "{cleaned}");
        assert!(!cleaned.contains('#'), "{cleaned}");
        assert!(cleaned.contains("rustlang"), "{cleaned}");
        assert!(!cleaned.contains('🚀'), "{cleaned}");
        assert!(!cleaned.contains('👍'), "{cleaned}");
        // CJK must survive emoji stripping
        assert!(cleaned.contains("日本語のテキストも大丈夫"), "{cleaned}");
        assert!(cleaned.contains("Check this out"), "{cleaned}");
    }

    #[test]
    fn markdown_issue_profile_cleans_github_issue() {
        let issue = "### Describe the bug\n\
            <!-- Please describe what happened -->\n\
            The **parser** crashes on [empty input](https://github.com/x/y/issues/1).\n\
            \n\
            ### Steps to reproduce\n\
            - [x] run the tool\n\
            ```rust\n\
            fn main() { panic!(\"boom\"); }\n\
            ```\n\
            ### Expected behaviour\n\
            No crash.";
        let cleaned = clean_for_embedding(issue, CleaningProfile::MarkdownIssue).unwrap();
        assert!(!cleaned.contains("```"), "{cleaned}");
        assert!(
            !cleaned.contains("panic!"),
            "code fence content must go: {cleaned}"
        );
        assert!(!cleaned.contains("Describe the bug"), "{cleaned}");
        assert!(!cleaned.contains("Steps to reproduce"), "{cleaned}");
        assert!(!cleaned.contains("<!--"), "{cleaned}");
        assert!(!cleaned.contains("**"), "{cleaned}");
        assert!(cleaned.contains("parser"), "{cleaned}");
        assert!(
            cleaned.contains("empty input"),
            "link label must stay: {cleaned}"
        );
        assert!(cleaned.contains("No crash"), "{cleaned}");
    }

    #[test]
    fn too_thin_text_is_rejected() {
        assert!(clean_for_embedding("👍", CleaningProfile::Social).is_none());
        assert!(clean_for_embedding("https://a.io/b", CleaningProfile::Social).is_none());
        assert!(clean_for_embedding("ok", CleaningProfile::Plain).is_none());
        assert!(clean_for_embedding("", CleaningProfile::Plain).is_none());
    }

    #[test]
    fn short_cjk_text_is_kept() {
        // 8 CJK chars, zero whitespace tokens beyond one
        assert!(clean_for_embedding("これは短い投稿です", CleaningProfile::Social).is_some());
    }

    #[test]
    fn unterminated_code_fence_is_stripped() {
        let t = "prefix words here\n```\nlet x = 1;\nnever closed";
        let cleaned = clean(t, CleaningProfile::MarkdownIssue);
        assert!(!cleaned.contains("let x"), "{cleaned}");
        assert!(cleaned.contains("prefix words here"), "{cleaned}");
    }

    #[test]
    fn effective_model_id_includes_profile_version() {
        assert_eq!(
            effective_model_id("potion-base-32M", CleaningProfile::Social),
            "potion-base-32M#clean=social.v1"
        );
    }

    #[test]
    fn profile_parse_round_trip() {
        for p in [
            CleaningProfile::Social,
            CleaningProfile::MarkdownIssue,
            CleaningProfile::Plain,
        ] {
            assert_eq!(CleaningProfile::parse(p.name()), Some(p));
        }
        assert_eq!(CleaningProfile::parse("bogus"), None);
    }
}

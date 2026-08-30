//! Deterministic, pre-call language identification.
//!
//! Language is an *input* to ticket enrichment, not an LLM output: per-language
//! cost accounting and per-language launch gating both need it before the
//! call is made, and a library answers in a millisecond without ever
//! inventing a code. Output is the BCP-47 primary subtag (ISO 639-1 where one
//! exists, else the ISO 639-3 code whatlang reports) so it can be compared
//! against source-provided `lang` columns directly.

/// Texts shorter than this return `None`: the detector's priors dominate and
/// a one-line "ok" is not evidence of anything.
pub const MIN_DETECT_CHARS: usize = 20;

/// Detect the language of `text`, or `None` when it is too short or the
/// detector is not confident. Returns a lowercase primary subtag.
pub fn detect_language(text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    if trimmed.chars().count() < MIN_DETECT_CHARS {
        return None;
    }
    let info = whatlang::detect(trimmed)?;
    if !info.is_reliable() {
        return None;
    }
    Some(primary_subtag(info.lang()))
}

/// ISO 639-3 → ISO 639-1 for every language whatlang knows; the 639-3 code
/// itself when no two-letter code exists (still a valid BCP-47 subtag).
fn primary_subtag(lang: whatlang::Lang) -> &'static str {
    use whatlang::Lang as L;
    match lang {
        L::Epo => "eo",
        L::Eng => "en",
        L::Rus => "ru",
        L::Cmn => "zh",
        L::Spa => "es",
        L::Por => "pt",
        L::Ita => "it",
        L::Ben => "bn",
        L::Fra => "fr",
        L::Deu => "de",
        L::Ukr => "uk",
        L::Kat => "ka",
        L::Ara => "ar",
        L::Hin => "hi",
        L::Jpn => "ja",
        L::Heb => "he",
        L::Yid => "yi",
        L::Pol => "pl",
        L::Amh => "am",
        L::Jav => "jv",
        L::Kor => "ko",
        L::Nob => "nb",
        L::Dan => "da",
        L::Swe => "sv",
        L::Fin => "fi",
        L::Tur => "tr",
        L::Nld => "nl",
        L::Hun => "hu",
        L::Ces => "cs",
        L::Ell => "el",
        L::Bul => "bg",
        L::Bel => "be",
        L::Mar => "mr",
        L::Kan => "kn",
        L::Ron => "ro",
        L::Slv => "sl",
        L::Hrv => "hr",
        L::Srp => "sr",
        L::Mkd => "mk",
        L::Lit => "lt",
        L::Lav => "lv",
        L::Est => "et",
        L::Tam => "ta",
        L::Vie => "vi",
        L::Urd => "ur",
        L::Tha => "th",
        L::Guj => "gu",
        L::Uzb => "uz",
        L::Pan => "pa",
        L::Aze => "az",
        L::Ind => "id",
        L::Tel => "te",
        L::Pes => "fa",
        L::Mal => "ml",
        L::Ori => "or",
        L::Mya => "my",
        L::Nep => "ne",
        L::Sin => "si",
        L::Khm => "km",
        L::Tuk => "tk",
        L::Aka => "ak",
        L::Zul => "zu",
        L::Sna => "sn",
        L::Afr => "af",
        L::Lat => "la",
        L::Slk => "sk",
        L::Cat => "ca",
        L::Tgl => "tl",
        L::Hye => "hy",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_three_launch_languages() {
        assert_eq!(
            detect_language(
                "The invoice does not show the VAT breakdown for customers in the European Union."
            ),
            Some("en")
        );
        assert_eq!(
            detect_language(
                "Fakturan visar inte momsen separat för kunder inom EU och beloppet blir därför fel."
            ),
            Some("sv")
        );
        assert_eq!(
            detect_language(
                "Lasku ei näytä arvonlisäveroa erikseen EU-asiakkaille ja summa on siksi väärin."
            ),
            Some("fi")
        );
    }

    #[test]
    fn short_text_is_not_evidence() {
        assert_eq!(detect_language("ok thanks"), None);
        assert_eq!(detect_language("   "), None);
        // Exactly the floor counts; one under does not.
        let twenty = "a".repeat(MIN_DETECT_CHARS);
        let _ = detect_language(&twenty); // may be None (unreliable), must not panic
        assert_eq!(detect_language(&"a".repeat(MIN_DETECT_CHARS - 1)), None);
    }

    #[test]
    fn code_only_bodies_do_not_get_a_confident_language() {
        // A stack trace / code dump: either None or something the caller can
        // override — the contract is only that it never panics and never
        // returns an empty tag.
        let code =
            "fn main() { let x = vec![1, 2, 3]; let total: i32 = x.iter().sum(); dbg!(total); }";
        if let Some(tag) = detect_language(code) {
            assert!(!tag.is_empty());
        }
    }
}

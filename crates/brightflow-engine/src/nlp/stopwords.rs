//! English stopword set for term scoring (Text Explorer's words panel).
//!
//! The crate list plus the fragments tokenization leaves behind from
//! contractions and GitHub-issue prose; without those, "ve" and "didn" rank
//! as if they were words.

use std::collections::HashSet;

/// Build the English stopword set.
pub fn english_stopwords() -> HashSet<String> {
    let mut set: HashSet<String> = stop_words::get(stop_words::LANGUAGE::English)
        .into_iter()
        .collect();
    for extra in [
        "ve", "re", "ll", "d", "m", "s", "t", "n", "didn", "doesn", "isn", "wasn", "wouldn",
        "couldn", "shouldn", "won", "im", "ive", "ill", "dont", "cant", "wont", "thats",
    ] {
        set.insert(extra.to_string());
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_crate_words_and_tokenizer_fragments() {
        let set = english_stopwords();
        assert!(set.contains("the"));
        assert!(set.contains("didn"));
        assert!(!set.contains("invoice"));
    }
}

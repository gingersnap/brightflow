//! Per-table document display schema for the topics API.
//!
//! Presentation-only concern: maps a table type to the columns used when
//! rendering cluster samples (id, headline, link, tags). Embedding/clustering
//! configuration lives in `brightflow_engine::enrichment::EnrichmentConfig`.

/// How to produce a document's external link.
pub(crate) enum UrlSpec {
    /// Read the URL from a column verbatim.
    Column(&'static str),
    /// Derive a bsky.app permalink from `uri` + `author_handle`/`author_did`.
    BlueskyPost,
    /// No link available for this table type.
    None,
}

/// Columns used to render one document in cluster details.
pub(crate) struct DocDisplay {
    /// Stringified into `DocRef.id`.
    pub id_column: &'static str,
    pub number_column: Option<&'static str>,
    /// None = derive a headline from the body's first line.
    pub title_column: Option<&'static str>,
    pub body_column: Option<&'static str>,
    pub url: UrlSpec,
    /// Comma-joined tags (GitHub labels, Bluesky hashtags).
    pub label_column: Option<&'static str>,
    pub timestamp_column: &'static str,
}

impl DocDisplay {
    /// Display schema for a table type; total — unknown tables get a
    /// minimal fallback that degrades to bodiless, linkless refs.
    //
    // KNOWN DUAL SOURCE OF TRUTH: `issues` hardcodes `label_names` as the tag
    // column here, while the engine writes `predicted_labels` from the trained
    // head. These are two different columns with two different producers.
    // `label_names` is the raw comma-encoded source column the connector ships
    // (GitHub labels); `predicted_labels` is the supervised classifier's
    // output. They are not unified — the topics UI reads `predicted_labels`,
    // the cluster-sample renderer reads `label_names` via this mapping. Keep
    // both in mind when changing which column a surface keys off.
    pub fn for_table(table_name: &str) -> Self {
        match table_name {
            "issues" => Self {
                id_column: "id",
                number_column: Some("number"),
                title_column: Some("title"),
                body_column: Some("body"),
                url: UrlSpec::Column("html_url"),
                label_column: Some("label_names"),
                timestamp_column: "created_at",
            },
            "posts" => Self {
                id_column: "uri",
                number_column: None,
                title_column: None,
                body_column: Some("text"),
                url: UrlSpec::BlueskyPost,
                label_column: Some("hashtags"),
                timestamp_column: "created_at",
            },
            _ => Self {
                id_column: "id",
                number_column: None,
                title_column: None,
                body_column: None,
                url: UrlSpec::None,
                label_column: None,
                timestamp_column: "created_at",
            },
        }
    }
}

/// Build a bsky.app permalink from an AT-URI plus author handle/DID.
///
/// `at://did:plc:xyz/app.bsky.feed.post/3kabc` →
/// `https://bsky.app/profile/{handle-or-did}/post/3kabc`.
/// The connector emits `""` for a missing handle, so fall back to the DID;
/// malformed input yields None (the UI simply renders no link).
pub(crate) fn bluesky_post_url(
    uri: &str,
    handle: Option<&str>,
    did: Option<&str>,
) -> Option<String> {
    if !uri.contains("/app.bsky.feed.post/") {
        return None;
    }
    let rkey = uri.rsplit('/').next().filter(|s| !s.is_empty())?;
    let profile = handle
        .filter(|h| !h.is_empty())
        .or_else(|| did.filter(|d| !d.is_empty()))?;
    Some(format!("https://bsky.app/profile/{profile}/post/{rkey}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_mapping() {
        let d = DocDisplay::for_table("issues");
        assert_eq!(d.id_column, "id");
        assert_eq!(d.number_column, Some("number"));
        assert_eq!(d.title_column, Some("title"));
        assert_eq!(d.body_column, Some("body"));
        assert!(matches!(d.url, UrlSpec::Column("html_url")));
        assert_eq!(d.label_column, Some("label_names"));
        assert_eq!(d.timestamp_column, "created_at");
    }

    #[test]
    fn posts_mapping() {
        let d = DocDisplay::for_table("posts");
        assert_eq!(d.id_column, "uri");
        assert_eq!(d.number_column, None);
        assert_eq!(d.title_column, None);
        assert_eq!(d.body_column, Some("text"));
        assert!(matches!(d.url, UrlSpec::BlueskyPost));
        assert_eq!(d.label_column, Some("hashtags"));
    }

    #[test]
    fn unknown_table_fallback() {
        let d = DocDisplay::for_table("users");
        assert_eq!(d.id_column, "id");
        assert_eq!(d.title_column, None);
        assert_eq!(d.body_column, None);
        assert!(matches!(d.url, UrlSpec::None));
        assert_eq!(d.label_column, None);
    }

    #[test]
    fn permalink_happy_path() {
        let url = bluesky_post_url(
            "at://did:plc:abc123/app.bsky.feed.post/3kxyz",
            Some("alice.bsky.social"),
            Some("did:plc:abc123"),
        );
        assert_eq!(
            url.as_deref(),
            Some("https://bsky.app/profile/alice.bsky.social/post/3kxyz")
        );
    }

    #[test]
    fn permalink_empty_handle_falls_back_to_did() {
        let url = bluesky_post_url(
            "at://did:plc:abc123/app.bsky.feed.post/3kxyz",
            Some(""),
            Some("did:plc:abc123"),
        );
        assert_eq!(
            url.as_deref(),
            Some("https://bsky.app/profile/did:plc:abc123/post/3kxyz")
        );
    }

    #[test]
    fn permalink_non_post_uri_is_none() {
        assert!(bluesky_post_url(
            "at://did:plc:abc123/app.bsky.feed.like/3kxyz",
            Some("alice.bsky.social"),
            None,
        )
        .is_none());
    }

    #[test]
    fn permalink_malformed_is_none() {
        // Empty rkey after the last slash
        assert!(bluesky_post_url(
            "at://did:plc:abc123/app.bsky.feed.post/",
            Some("alice.bsky.social"),
            None,
        )
        .is_none());
        // No handle and no DID
        assert!(bluesky_post_url(
            "at://did:plc:abc123/app.bsky.feed.post/3kxyz",
            Some(""),
            None,
        )
        .is_none());
        assert!(bluesky_post_url("", None, None).is_none());
    }
}

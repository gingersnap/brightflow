//! Canonical source-id and event-table naming shared across crates.
//!
//! A store source id is `"{kind}:{id}"` — web tracking sites, connector
//! configs, and one-off uploads each prefix their own id — and a web source's
//! event table is `"events_{id}"`. These strings appear in the store catalog,
//! on disk, and over the API, so the formats are pinned here (with
//! literal-pinning tests) instead of being re-derived by hand at each call
//! site. Pure `format!` only: this crate stays dependency-free.

/// Store source id for a web-analytics tracking site.
#[must_use]
pub fn web_source_id(site_id: &str) -> String {
    format!("web:{site_id}")
}

/// Store source id for a connector config.
#[must_use]
pub fn connector_source_id(config_id: &str) -> String {
    format!("connector:{config_id}")
}

/// Store source id for a one-off file upload.
#[must_use]
pub fn upload_source_id(upload_id: &str) -> String {
    format!("upload:{upload_id}")
}

/// Name of the event table holding a web source's flushed events.
#[must_use]
pub fn events_table_name(site_id: &str) -> String {
    format!("events_{site_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test pins the literal format: these strings live in catalogs and
    // on disk, so a change here is a data migration, not a refactor.

    #[test]
    fn web_source_id_literal() {
        assert_eq!(web_source_id("abc"), "web:abc");
    }

    #[test]
    fn connector_source_id_literal() {
        assert_eq!(connector_source_id("abc"), "connector:abc");
    }

    #[test]
    fn upload_source_id_literal() {
        assert_eq!(upload_source_id("abc"), "upload:abc");
    }

    #[test]
    fn events_table_name_literal() {
        assert_eq!(events_table_name("abc"), "events_abc");
    }
}

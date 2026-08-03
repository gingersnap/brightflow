//! Rejects path traversal in dynamic route segments before they reach a handler.
//!
//! Several handlers build filesystem paths by joining route parameters —
//! `source_id`, `table`, `name` — straight onto the workspace root (directly, via
//! `topics_artifact_dir`, and inside `brightflow-store`, which does
//! `root.join(source_id).join(name)` in a dozen places). axum percent-decodes a
//! segment before handing it over, so `%2F` becomes a real `/` and `..%2F..` a
//! real `../..`; nothing downstream re-checks it. A logged-in client could thus
//! read or write outside the workspace (confirmed against a running server: a
//! crafted `source_id` made the topics endpoint stat a planted file under
//! `/tmp`).
//!
//! This is the one choke point that closes it. As a middleware over the whole
//! router it validates *every* dynamic segment of *every* route — so a new
//! handler cannot reintroduce the hole by forgetting to sanitize, which per-site
//! checks invite. Validation happens before the value can propagate to either the
//! API path builders or the store, which is why it belongs here rather than at
//! each sink.
//!
//! The rule is deliberately blunt: a single decoded segment that is exactly `.`
//! or `..`, or that contains a path separator or NUL, cannot be a legitimate
//! `source_id`/`table`/`name` (those look like `connector:abc` or `issues`) and
//! is the only shape that can traverse. Everything else passes untouched.

use axum::extract::RawPathParams;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{extract::Request, middleware::Next};

/// Whether a single decoded path segment could escape the directory it is joined
/// into.
///
/// True for the exact components `.` and `..`, and for anything carrying a path
/// separator (`/` or `\`) or a NUL. A `/` only reaches here because axum already
/// decoded a `%2F`, so treating it as hostile is correct: a real route separator
/// would have been split into its own segment.
fn is_unsafe_component(value: &str) -> bool {
    value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
}

/// Middleware: 400 if any dynamic route segment could traverse, else continue.
///
/// Applied as a `route_layer` on both routers so it runs for every matched route
/// regardless of auth. Numeric segments (e.g. `cluster_id`) and identity-only
/// ones stringify to values that never contain a separator, so they pass without
/// being special-cased.
pub async fn reject_unsafe_path_params(
    params: RawPathParams,
    request: Request,
    next: Next,
) -> Response {
    for (_key, value) in &params {
        if is_unsafe_component(value) {
            return (
                StatusCode::BAD_REQUEST,
                "path parameter contains an illegal character",
            )
                .into_response();
        }
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legitimate_ids_and_names_pass() {
        // The real shapes: prefixed source ids and bare table names.
        for ok in [
            "connector:abc",
            "web:1234",
            "upload:0190f2c0-7e5a-7000-8000-000000000000",
            "issues",
            "enriched",
            "events",
            "a_table-name.v2",
        ] {
            assert!(!is_unsafe_component(ok), "{ok} should be allowed");
        }
    }

    #[test]
    fn traversal_shapes_are_rejected() {
        // `..` as a whole segment traverses up one level when joined.
        assert!(is_unsafe_component(".."));
        assert!(is_unsafe_component("."));
        // A decoded %2F carries a separator into a single segment.
        assert!(is_unsafe_component("../../etc"));
        assert!(is_unsafe_component("..\\..\\windows"));
        assert!(is_unsafe_component("/etc/passwd"));
        // NUL could truncate a path in a downstream C call.
        assert!(is_unsafe_component("a\0b"));
    }

    #[test]
    fn double_dots_inside_a_name_without_a_separator_are_allowed() {
        // "a..b" cannot traverse on its own — it is a single directory name.
        // Only an exact ".." or a separator escapes, so this must not be a false
        // positive that breaks a legitimately odd table name.
        assert!(!is_unsafe_component("a..b"));
        assert!(!is_unsafe_component("v1..v2"));
    }
}

use std::path::{Path, PathBuf};

/// Directory name for the bundled Model2Vec encoder.
/// Doubles as the `embedding_model_id` written to parquet rows.
pub const POTION_BASE_32M_DIR: &str = "potion-base-32M";

/// Filesystem path of the embedder model directory.
///
/// May be overridden with the `BRIGHTFLOW_EMBEDDER_PATH` env var (used in tests).
pub fn embedder_path(workspace_root: &Path) -> PathBuf {
    if let Ok(override_path) = std::env::var("BRIGHTFLOW_EMBEDDER_PATH") {
        return PathBuf::from(override_path);
    }
    workspace_root
        .join("models")
        .join("embedder")
        .join(POTION_BASE_32M_DIR)
}

/// Per-source artifact directory: `{workspace}/models/sources/{sanitized_id}/{table}/`.
pub fn topics_artifact_dir(workspace_root: &Path, source_id: &str, table_name: &str) -> PathBuf {
    workspace_root
        .join("models")
        .join("sources")
        .join(sanitize_source_id(source_id))
        .join(table_name)
}

/// Source IDs may contain `:` (e.g. `connector:gh-foo`) which is not a valid path
/// segment on Windows. Replace with a double underscore for filesystem safety.
pub fn sanitize_source_id(source_id: &str) -> String {
    source_id.replace(':', "__")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_colon() {
        assert_eq!(sanitize_source_id("connector:abc"), "connector__abc");
        assert_eq!(sanitize_source_id("plain"), "plain");
    }

    #[test]
    fn artifact_dir_layout() {
        let p = topics_artifact_dir(Path::new("/ws"), "connector:gh", "issues");
        assert!(p.ends_with("models/sources/connector__gh/issues"));
    }
}

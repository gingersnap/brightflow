//! Brightflow's face on Longbow: connector discovery and pipeline runs.
//!
//! Connectors are Lua sources — some embedded in the binary as builtins, some
//! loaded from a custom directory — so adding or patching one never requires
//! a Rust rebuild of the connector itself. On a name collision, discovery
//! keeps the builtin and skips the custom file.

pub use longbow;

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

/// Error from loading or running a connector.
///
/// Always a fully rendered message: every failure here is terminal for the
/// run and callers only display it, so a structured hierarchy would carry
/// no information anyone reads programmatically.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ConnectError(pub String);

pub type Result<T> = std::result::Result<T, ConnectError>;

/// A connector discovered from builtins or the filesystem.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AvailableConnector {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub source_type: String, // "builtin" or "custom"
    pub source_hash: String,
}

/// Discover all available connectors from builtins and an optional custom directory.
pub fn discover_connectors(custom_dir: Option<&Path>) -> Vec<AvailableConnector> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. Builtins
    for (name, source) in BUILTIN_CONNECTORS.iter() {
        let meta = longbow::pipeline::parse_frontmatter(source);
        let display_name = meta.name.clone().unwrap_or_else(|| (*name).to_string());
        seen.insert(display_name.clone());
        result.push(AvailableConnector {
            name: display_name,
            version: meta.version,
            description: meta.description,
            source_type: "builtin".to_string(),
            source_hash: meta.source_hash,
        });
    }

    // 2. Custom directory
    if let Some(dir) = custom_dir {
        if dir.exists() && dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "lua") {
                        if let Ok(source) = std::fs::read_to_string(&path) {
                            let meta = longbow::pipeline::parse_frontmatter(&source);
                            let name = meta.name.clone().unwrap_or_else(|| {
                                path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown")
                                    .to_string()
                            });
                            if !seen.contains(&name) {
                                seen.insert(name.clone());
                                result.push(AvailableConnector {
                                    name,
                                    version: meta.version,
                                    description: meta.description,
                                    source_type: "custom".to_string(),
                                    source_hash: meta.source_hash,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

/// Built-in connector Lua sources, embedded at compile time.
static BUILTIN_CONNECTORS: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("github", include_str!("../connectors/github.lua"));
    m.insert("bluesky", include_str!("../connectors/bluesky.lua"));
    m
});

/// Options for running a connector
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Only sync specific endpoints (comma-separated)
    pub only: Option<String>,
    /// Dry run mode - don't actually fetch data
    pub dry_run: bool,
    /// Cursor values for incremental sync: endpoint -> cursor_value
    pub cursor_values: HashMap<String, String>,
}

/// Result of running a connector
#[derive(Debug, Clone)]
pub struct ConnectorResult {
    pub endpoints: Vec<EndpointResultInfo>,
    pub dry_run: bool,
    pub output_path: String,
    pub duration_ms: u64,
}

/// Per-endpoint metadata from a connector run
#[derive(Debug, Clone)]
pub struct EndpointResultInfo {
    pub name: String,
    pub parquet_path: Option<String>,
    pub rows: usize,
    pub primary_key: Vec<String>,
    pub cursor_field: Option<String>,
    pub cursor_value: Option<String>,
    pub duration_ms: u64,
}

/// Filter pipeline endpoints if --only is specified
fn apply_endpoint_filter(pipeline: &mut longbow::pipeline::Pipeline, only: Option<&String>) {
    if let Some(only_endpoints) = only {
        let names: Vec<&str> = only_endpoints.split(',').map(str::trim).collect();
        pipeline
            .endpoints
            .retain(|e| names.contains(&e.name.as_str()));
    }
}

/// Build a dry-run result from a pipeline (no execution happened)
fn build_dry_run_result(pipeline: &longbow::pipeline::Pipeline) -> ConnectorResult {
    ConnectorResult {
        endpoints: pipeline
            .endpoints
            .iter()
            .map(|e| EndpointResultInfo {
                name: e.name.clone(),
                parquet_path: None,
                rows: 0,
                primary_key: e.primary_key.clone(),
                cursor_field: e.cursor_field.clone(),
                cursor_value: None,
                duration_ms: 0,
            })
            .collect(),
        dry_run: true,
        output_path: pipeline
            .output
            .as_ref()
            .map(|o| o.path.clone())
            .unwrap_or_default(),
        duration_ms: 0,
    }
}

/// Map a Longbow `RunResult` into our `ConnectorResult`
fn map_run_result(
    run_result: longbow::RunResult,
    output_path: String,
    dry_run: bool,
) -> ConnectorResult {
    ConnectorResult {
        endpoints: run_result
            .endpoints
            .into_iter()
            .map(|ep| EndpointResultInfo {
                name: ep.name,
                parquet_path: ep.parquet_path,
                rows: ep.rows,
                primary_key: ep.primary_key,
                cursor_field: ep.cursor_field,
                cursor_value: ep.cursor_value,
                duration_ms: ep.duration_ms,
            })
            .collect(),
        dry_run,
        output_path,
        duration_ms: run_result.duration_ms,
    }
}

/// Run a connector with the given configuration (file-based config).
pub async fn run_connector(
    connector_path: &Path,
    config_path: &Path,
    options: &RunOptions,
) -> Result<ConnectorResult> {
    let config_str = config_path
        .to_str()
        .ok_or_else(|| ConnectError("Invalid config path".to_string()))?;
    let config = longbow::config::load_config(config_str)
        .map_err(|e| ConnectError(format!("Failed to load config: {e}")))?;

    let lua = longbow::runtime::create_lua_runtime()
        .map_err(|e| ConnectError(format!("Failed to create Lua runtime: {e}")))?;

    let connector_str = connector_path
        .to_str()
        .ok_or_else(|| ConnectError("Invalid connector path".to_string()))?;
    let mut pipeline = longbow::pipeline::load_connector(&lua, connector_str, config)
        .map_err(|e| ConnectError(format!("Failed to load connector: {e}")))?;

    apply_endpoint_filter(&mut pipeline, options.only.as_ref());

    if options.dry_run {
        return Ok(build_dry_run_result(&pipeline));
    }

    let output_path = pipeline
        .output
        .as_ref()
        .map(|o| o.path.clone())
        .unwrap_or_default();

    let http = longbow::http::HttpClient::new();
    let run_result = longbow::pipeline::execute(&pipeline, &lua, &http)
        .await
        .map_err(|e| ConnectError(format!("Pipeline execution failed: {e}")))?;

    Ok(map_run_result(run_result, output_path, false))
}

/// Run a connector with config passed directly as JSON (no file I/O).
/// This is the primary API when Brightflow passes config from SQLite.
pub async fn run_connector_with_config(
    connector_path: &Path,
    config: serde_json::Value,
    options: &RunOptions,
) -> Result<ConnectorResult> {
    run_connector_impl(ConnectorSource::Path(connector_path), config, options).await
}

/// Run a connector from embedded Lua source with config passed as JSON.
pub async fn run_connector_from_source(
    lua_source: &str,
    config: serde_json::Value,
    options: &RunOptions,
) -> Result<ConnectorResult> {
    run_connector_impl(ConnectorSource::Source(lua_source), config, options).await
}

/// Internal: either a filesystem path or inline Lua source.
enum ConnectorSource<'a> {
    Path(&'a Path),
    Source(&'a str),
}

/// Shared implementation for running a connector with JSON config.
async fn run_connector_impl(
    source: ConnectorSource<'_>,
    mut config: serde_json::Value,
    options: &RunOptions,
) -> Result<ConnectorResult> {
    // Inject cursor values into the config
    if !options.cursor_values.is_empty() {
        if let Some(obj) = config.as_object_mut() {
            let cursors = serde_json::to_value(&options.cursor_values)
                .map_err(|e| ConnectError(e.to_string()))?;
            obj.insert("_cursors".to_string(), cursors);
        }
    }

    let lua = longbow::runtime::create_lua_runtime()
        .map_err(|e| ConnectError(format!("Failed to create Lua runtime: {e}")))?;

    let mut pipeline = match source {
        ConnectorSource::Path(path) => {
            let connector_str = path
                .to_str()
                .ok_or_else(|| ConnectError("Invalid connector path".to_string()))?;
            longbow::pipeline::load_connector(&lua, connector_str, config)
        },
        ConnectorSource::Source(lua_src) => {
            longbow::pipeline::load_connector_from_source(&lua, lua_src, config)
        },
    }
    .map_err(|e| ConnectError(format!("Failed to load connector: {e}")))?;

    apply_endpoint_filter(&mut pipeline, options.only.as_ref());

    if options.dry_run {
        return Ok(build_dry_run_result(&pipeline));
    }

    let output_path = pipeline
        .output
        .as_ref()
        .map(|o| o.path.clone())
        .unwrap_or_default();

    let http = longbow::http::HttpClient::new();
    let run_result = longbow::pipeline::execute(&pipeline, &lua, &http)
        .await
        .map_err(|e| ConnectError(format!("Pipeline execution failed: {e}")))?;

    Ok(map_run_result(run_result, output_path, false))
}

/// List available built-in connectors (embedded at compile time).
pub fn list_builtin_connectors() -> Vec<String> {
    BUILTIN_CONNECTORS
        .keys()
        .map(|k| (*k).to_string())
        .collect()
}

/// Get the embedded Lua source for a built-in connector by name.
pub fn get_builtin_connector_source(name: &str) -> Option<&'static str> {
    BUILTIN_CONNECTORS.get(name).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_error_displays_its_message_verbatim() {
        let err = ConnectError("pipeline exploded".to_string());
        assert_eq!(err.to_string(), "pipeline exploded");
        // It must remain a std error so callers can box/`?` it.
        let _: &dyn std::error::Error = &err;
    }

    // -- discover_connectors ------------------------------------------------

    /// Write `contents` as `<dir>/<file_name>` and return the directory path.
    fn write_lua(dir: &Path, file_name: &str, contents: &str) {
        std::fs::write(dir.join(file_name), contents).unwrap();
    }

    #[test]
    fn discovery_without_custom_dir_lists_only_builtins() {
        let found = discover_connectors(None);
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"github"));
        assert!(names.contains(&"bluesky"));
        assert!(found.iter().all(|c| c.source_type == "builtin"));
    }

    #[test]
    fn discovery_keeps_builtin_on_name_collision_and_adds_custom_files() {
        let dir = tempfile::tempdir().unwrap();

        // Collides with the builtin "github" via its frontmatter name.
        write_lua(
            dir.path(),
            "my_github.lua",
            "--[[ @longbow\nname = \"github\"\nversion = \"9.9.9\"\n]]\nreturn function(p) end\n",
        );
        // Does not collide; no frontmatter name, so the file stem is the name.
        write_lua(dir.path(), "acme.lua", "return function(p) end\n");
        // Wrong extension: must be ignored entirely.
        write_lua(dir.path(), "notes.txt", "not a connector");

        let found = discover_connectors(Some(dir.path()));

        // Exactly one "github" survives, and it is the builtin: its hash is
        // the builtin source's hash, not the custom file's.
        let githubs: Vec<_> = found.iter().filter(|c| c.name == "github").collect();
        assert_eq!(githubs.len(), 1);
        assert_eq!(githubs[0].source_type, "builtin");
        let builtin_hash =
            longbow::pipeline::parse_frontmatter(get_builtin_connector_source("github").unwrap())
                .source_hash;
        assert_eq!(githubs[0].source_hash, builtin_hash);
        assert_ne!(githubs[0].version.as_deref(), Some("9.9.9"));

        // The non-colliding custom file is discovered under its file stem.
        let acme = found.iter().find(|c| c.name == "acme").unwrap();
        assert_eq!(acme.source_type, "custom");

        // The .txt file contributed nothing.
        assert_eq!(found.len(), discover_connectors(None).len() + 1);
    }

    #[test]
    fn discovery_ignores_a_missing_custom_dir() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("does-not-exist");
        assert_eq!(
            discover_connectors(Some(&gone)).len(),
            discover_connectors(None).len()
        );
    }

    // -- pipeline helpers ---------------------------------------------------

    /// Build a real `Pipeline` by evaluating an inline Lua connector — the
    /// same loader production uses — since longbow's `Pipeline`/`Endpoint`
    /// are `#[non_exhaustive]` and cannot be struct-literal'd here.
    fn pipeline_from_lua(source: &str) -> longbow::pipeline::Pipeline {
        let lua = longbow::runtime::create_lua_runtime().unwrap();
        longbow::pipeline::load_connector_from_source(&lua, source, serde_json::json!({})).unwrap()
    }

    /// A two-endpoint pipeline with a parquet output block.
    fn two_endpoint_pipeline() -> longbow::pipeline::Pipeline {
        pipeline_from_lua(
            r#"
            return function(p)
                p.base_url("https://example.test")
                p.output(output.parquet({ path = "/data/out" }))
                p.endpoint("issues", {
                    path = "/issues",
                    primary_key = { "org", "id" },
                    cursor_field = "updated_at",
                })
                p.endpoint("stars", { path = "/stars" })
            end
            "#,
        )
    }

    // -- apply_endpoint_filter ----------------------------------------------

    fn endpoint_names(pipeline: &longbow::pipeline::Pipeline) -> Vec<&str> {
        pipeline.endpoints.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn endpoint_filter_none_is_a_no_op() {
        let mut pipeline = two_endpoint_pipeline();
        apply_endpoint_filter(&mut pipeline, None);
        assert_eq!(endpoint_names(&pipeline), ["issues", "stars"]);
    }

    #[test]
    fn endpoint_filter_retains_only_named_endpoints() {
        let mut pipeline = two_endpoint_pipeline();
        apply_endpoint_filter(&mut pipeline, Some(&"stars".to_string()));
        assert_eq!(endpoint_names(&pipeline), ["stars"]);
    }

    #[test]
    fn endpoint_filter_splits_on_commas_and_trims_whitespace() {
        let mut pipeline = two_endpoint_pipeline();
        apply_endpoint_filter(&mut pipeline, Some(&" stars ,  issues".to_string()));
        // retain() preserves pipeline order regardless of filter order.
        assert_eq!(endpoint_names(&pipeline), ["issues", "stars"]);
    }

    #[test]
    fn endpoint_filter_with_unknown_name_empties_the_pipeline() {
        let mut pipeline = two_endpoint_pipeline();
        apply_endpoint_filter(&mut pipeline, Some(&"nope".to_string()));
        assert!(pipeline.endpoints.is_empty());
    }

    // -- build_dry_run_result -----------------------------------------------

    #[test]
    fn dry_run_result_carries_endpoint_shape_with_zeroed_execution_fields() {
        let result = build_dry_run_result(&two_endpoint_pipeline());

        assert!(result.dry_run);
        assert_eq!(result.output_path, "/data/out");
        assert_eq!(result.duration_ms, 0);
        assert_eq!(result.endpoints.len(), 2);

        let issues = &result.endpoints[0];
        assert_eq!(issues.name, "issues");
        assert_eq!(issues.primary_key, ["org", "id"]);
        assert_eq!(issues.cursor_field.as_deref(), Some("updated_at"));
        // Nothing executed, so no artifacts and no counters.
        assert_eq!(issues.parquet_path, None);
        assert_eq!(issues.rows, 0);
        assert_eq!(issues.cursor_value, None);
        assert_eq!(issues.duration_ms, 0);

        let stars = &result.endpoints[1];
        assert_eq!(stars.name, "stars");
        // longbow defaults primary_key to ["id"] when the connector omits it.
        assert_eq!(stars.primary_key, ["id"]);
        assert_eq!(stars.cursor_field, None);
    }

    #[test]
    fn dry_run_result_output_path_is_empty_without_an_output_block() {
        let pipeline = pipeline_from_lua(
            r#"
            return function(p)
                p.endpoint("issues", { path = "/issues" })
            end
            "#,
        );
        let result = build_dry_run_result(&pipeline);
        assert_eq!(result.output_path, "");
    }

    // -- map_run_result -----------------------------------------------------

    #[test]
    fn map_run_result_translates_every_field_verbatim() {
        let run_result = longbow::RunResult {
            meta: longbow::pipeline::ConnectorMeta::default(),
            endpoints: vec![longbow::pipeline::EndpointResult {
                name: "issues".to_string(),
                parquet_path: Some("/data/out/issues.parquet".to_string()),
                rows: 42,
                primary_key: vec!["id".to_string()],
                cursor_field: Some("updated_at".to_string()),
                cursor_value: Some("2026-08-01T00:00:00Z".to_string()),
                duration_ms: 7,
            }],
            duration_ms: 123,
        };

        let result = map_run_result(run_result, "/data/out".to_string(), false);

        assert!(!result.dry_run);
        assert_eq!(result.output_path, "/data/out");
        assert_eq!(result.duration_ms, 123);
        assert_eq!(result.endpoints.len(), 1);
        let ep = &result.endpoints[0];
        assert_eq!(ep.name, "issues");
        assert_eq!(ep.parquet_path.as_deref(), Some("/data/out/issues.parquet"));
        assert_eq!(ep.rows, 42);
        assert_eq!(ep.primary_key, ["id"]);
        assert_eq!(ep.cursor_field.as_deref(), Some("updated_at"));
        assert_eq!(ep.cursor_value.as_deref(), Some("2026-08-01T00:00:00Z"));
        assert_eq!(ep.duration_ms, 7);
    }

    #[test]
    fn map_run_result_passes_the_dry_run_flag_through() {
        let run_result = longbow::RunResult {
            meta: longbow::pipeline::ConnectorMeta::default(),
            endpoints: vec![],
            duration_ms: 0,
        };
        let result = map_run_result(run_result, String::new(), true);
        assert!(result.dry_run);
        assert!(result.endpoints.is_empty());
    }
}

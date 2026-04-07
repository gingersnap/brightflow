//! Brightflow Connect - Longbow configuration and custom connectors
//!
//! This crate provides Brightflow's deployment of Longbow, including:
//! - Configuration for data source connectors
//! - Custom Lua connectors specific to Brightflow
//! - Connector runner utilities

pub use longbow;

use brightflow_core::{BrightflowError, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

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
        .ok_or_else(|| BrightflowError::Other("Invalid config path".to_string()))?;
    let config = longbow::config::load_config(config_str)
        .map_err(|e| BrightflowError::Other(format!("Failed to load config: {e}")))?;

    let lua = longbow::runtime::create_lua_runtime()
        .map_err(|e| BrightflowError::Other(format!("Failed to create Lua runtime: {e}")))?;

    let connector_str = connector_path
        .to_str()
        .ok_or_else(|| BrightflowError::Other("Invalid connector path".to_string()))?;
    let mut pipeline = longbow::pipeline::load_connector(&lua, connector_str, config)
        .map_err(|e| BrightflowError::Other(format!("Failed to load connector: {e}")))?;

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
        .map_err(|e| BrightflowError::Other(format!("Pipeline execution failed: {e}")))?;

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
                .map_err(|e| BrightflowError::Other(e.to_string()))?;
            obj.insert("_cursors".to_string(), cursors);
        }
    }

    let lua = longbow::runtime::create_lua_runtime()
        .map_err(|e| BrightflowError::Other(format!("Failed to create Lua runtime: {e}")))?;

    let mut pipeline = match source {
        ConnectorSource::Path(path) => {
            let connector_str = path
                .to_str()
                .ok_or_else(|| BrightflowError::Other("Invalid connector path".to_string()))?;
            longbow::pipeline::load_connector(&lua, connector_str, config)
        },
        ConnectorSource::Source(lua_src) => {
            longbow::pipeline::load_connector_from_source(&lua, lua_src, config)
        },
    }
    .map_err(|e| BrightflowError::Other(format!("Failed to load connector: {e}")))?;

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
        .map_err(|e| BrightflowError::Other(format!("Pipeline execution failed: {e}")))?;

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

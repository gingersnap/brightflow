//! Brightflow Connect - Longbow configuration and custom connectors
//!
//! This crate provides Brightflow's deployment of Longbow, including:
//! - Configuration for data source connectors
//! - Custom Lua connectors specific to Brightflow
//! - Connector runner utilities

pub use longbow;

use brightflow_core::{BrightflowError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Configuration for a connector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub name: String,
    pub connector_path: PathBuf,
    pub config_path: PathBuf,
    pub output_path: PathBuf,
}

/// Load connector configuration from a TOML file
pub fn load_config(path: &Path) -> Result<ConnectorConfig> {
    let content = std::fs::read_to_string(path)?;
    toml::from_str(&content).map_err(|e| BrightflowError::Other(e.to_string()))
}

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

/// Filter pipeline endpoints if --only is specified
fn apply_endpoint_filter(pipeline: &mut longbow::pipeline::Pipeline, only: Option<&String>) {
    if let Some(only_endpoints) = only {
        let names: Vec<&str> = only_endpoints.split(',').map(str::trim).collect();
        pipeline
            .endpoints
            .retain(|e| names.contains(&e.name.as_str()));
    }
}

/// Extract result metadata from a pipeline
fn build_result(pipeline: &longbow::pipeline::Pipeline, dry_run: bool) -> ConnectorResult {
    ConnectorResult {
        endpoints_synced: pipeline.endpoints.iter().map(|e| e.name.clone()).collect(),
        dry_run,
        output_path: pipeline
            .output
            .as_ref()
            .map(|o| o.path.clone())
            .unwrap_or_default(),
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
        return Ok(build_result(&pipeline, true));
    }

    let http = longbow::http::HttpClient::new();
    longbow::pipeline::execute(&pipeline, &lua, &http)
        .await
        .map_err(|e| BrightflowError::Other(format!("Pipeline execution failed: {e}")))?;

    Ok(build_result(&pipeline, false))
}

/// Run a connector with config passed directly as JSON (no file I/O).
/// This is the primary API when Brightflow passes config from SQLite.
pub async fn run_connector_with_config(
    connector_path: &Path,
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

    let processed_config = longbow::config::load_config_from_value(config)
        .map_err(|e| BrightflowError::Other(format!("Failed to process config: {e}")))?;

    let lua = longbow::runtime::create_lua_runtime()
        .map_err(|e| BrightflowError::Other(format!("Failed to create Lua runtime: {e}")))?;

    let connector_str = connector_path
        .to_str()
        .ok_or_else(|| BrightflowError::Other("Invalid connector path".to_string()))?;
    let mut pipeline = longbow::pipeline::load_connector(&lua, connector_str, processed_config)
        .map_err(|e| BrightflowError::Other(format!("Failed to load connector: {e}")))?;

    apply_endpoint_filter(&mut pipeline, options.only.as_ref());

    if options.dry_run {
        return Ok(build_result(&pipeline, true));
    }

    let http = longbow::http::HttpClient::new();
    longbow::pipeline::execute(&pipeline, &lua, &http)
        .await
        .map_err(|e| BrightflowError::Other(format!("Pipeline execution failed: {e}")))?;

    Ok(build_result(&pipeline, false))
}

/// Result of running a connector
#[derive(Debug, Clone)]
pub struct ConnectorResult {
    /// Names of endpoints that were synced
    pub endpoints_synced: Vec<String>,
    /// Whether this was a dry run
    pub dry_run: bool,
    /// Output path where data was written
    pub output_path: String,
}

/// Get the path to the built-in connectors directory
pub fn builtin_connectors_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("connectors")
}

/// List available built-in connectors
pub fn list_builtin_connectors() -> Result<Vec<String>> {
    let dir = builtin_connectors_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut connectors = Vec::new();
    for dir_entry in std::fs::read_dir(&dir)? {
        let path = dir_entry?.path();
        if path.extension().is_some_and(|ext| ext == "lua") {
            if let Some(name) = path.file_stem() {
                connectors.push(name.to_string_lossy().to_string());
            }
        }
    }

    Ok(connectors)
}

/// Get the path to a built-in connector by name
pub fn get_builtin_connector_path(name: &str) -> Option<PathBuf> {
    let path = builtin_connectors_dir().join(format!("{name}.lua"));
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

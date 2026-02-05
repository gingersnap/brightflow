//! Brightflow Connect - Avon configuration and custom connectors
//!
//! This crate provides Brightflow's deployment of Avon, including:
//! - Configuration for data source connectors
//! - Custom Lua connectors specific to Brightflow
//! - Connector runner utilities

pub use avon;

use brightflow_core::{BrightflowError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration for a connector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub name: String,
    pub connector_path: PathBuf,
    pub config_path: PathBuf,
    pub output_path: PathBuf,
}

/// Load connector configuration from a YAML file
pub fn load_config(path: &Path) -> Result<ConnectorConfig> {
    let content = std::fs::read_to_string(path)?;
    serde_yaml::from_str(&content).map_err(|e| BrightflowError::Other(e.to_string()))
}

/// Options for running a connector
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Only sync specific endpoints (comma-separated)
    pub only: Option<String>,
    /// Dry run mode - don't actually fetch data
    pub dry_run: bool,
}

/// Run a connector with the given configuration
///
/// This executes the Avon pipeline defined in the Lua connector file.
pub async fn run_connector(
    connector_path: &Path,
    config_path: &Path,
    options: &RunOptions,
) -> Result<ConnectorResult> {
    // Load the YAML config with environment variable substitution
    let config_str = config_path
        .to_str()
        .ok_or_else(|| BrightflowError::Other("Invalid config path".to_string()))?;
    let config = avon::config::load_config(config_str)
        .map_err(|e| BrightflowError::Other(format!("Failed to load config: {e}")))?;

    // Create the Lua runtime
    let lua = avon::runtime::create_lua_runtime()
        .map_err(|e| BrightflowError::Other(format!("Failed to create Lua runtime: {e}")))?;

    // Load the connector
    let connector_str = connector_path
        .to_str()
        .ok_or_else(|| BrightflowError::Other("Invalid connector path".to_string()))?;
    let mut pipeline = avon::pipeline::load_connector(&lua, connector_str, config)
        .map_err(|e| BrightflowError::Other(format!("Failed to load connector: {e}")))?;

    // Filter endpoints if --only specified
    if let Some(only_endpoints) = &options.only {
        let names: Vec<&str> = only_endpoints.split(',').map(str::trim).collect();
        pipeline
            .endpoints
            .retain(|e| names.contains(&e.name.as_str()));
    }

    // Get endpoint names for the result
    let endpoint_names: Vec<String> = pipeline.endpoints.iter().map(|e| e.name.clone()).collect();

    if options.dry_run {
        return Ok(ConnectorResult {
            endpoints_synced: endpoint_names,
            dry_run: true,
            output_path: pipeline
                .output
                .as_ref()
                .map(|o| o.path.clone())
                .unwrap_or_default(),
        });
    }

    // Execute the pipeline
    let http = avon::http::HttpClient::new();
    avon::pipeline::execute(&pipeline, &lua, &http)
        .await
        .map_err(|e| BrightflowError::Other(format!("Pipeline execution failed: {e}")))?;

    Ok(ConnectorResult {
        endpoints_synced: endpoint_names,
        dry_run: false,
        output_path: pipeline
            .output
            .as_ref()
            .map(|o| o.path.clone())
            .unwrap_or_default(),
    })
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

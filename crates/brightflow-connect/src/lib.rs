//! Brightflow Connect - Avon configuration and custom connectors
//!
//! This crate provides Brightflow's deployment of Avon, including:
//! - Configuration for data source connectors
//! - Custom Lua connectors specific to Brightflow
//! - Connector runner utilities

pub use avon;

use brightflow_core::{BrightflowError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for a connector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub name: String,
    pub connector_path: PathBuf,
    pub config_path: PathBuf,
    pub output_path: PathBuf,
}

/// Load connector configuration from a YAML file
pub fn load_config(path: &std::path::Path) -> Result<ConnectorConfig> {
    let content = std::fs::read_to_string(path)?;
    serde_yaml::from_str(&content).map_err(|e| BrightflowError::Other(e.to_string()))
}

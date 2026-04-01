use std::path::{Path, PathBuf};

use thiserror::Error;

/// Core error type for Brightflow
#[derive(Debug, Error)]
pub enum BrightflowError {
    #[error("Dataset not found: {0}")]
    DatasetNotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BrightflowError>;

/// Resolved workspace paths — single source of truth for all data locations.
///
/// Each accessor checks an env var override first, then falls back to
/// `{base}/workspaces/{workspace}/{sub}`.
#[derive(Debug, Clone)]
pub struct WorkspacePaths {
    base: PathBuf,
    workspace: String,
}

impl WorkspacePaths {
    /// Create from environment variables.
    ///
    /// - `BRIGHTFLOW_DATA_DIR` → base path (default: `./data`)
    /// - `BRIGHTFLOW_WORKSPACE` → workspace name (default: `"default"`)
    #[must_use]
    pub fn from_env() -> Self {
        let base = std::env::var("BRIGHTFLOW_DATA_DIR")
            .map_or_else(|_| PathBuf::from("./data"), PathBuf::from);
        let workspace = std::env::var("BRIGHTFLOW_WORKSPACE").unwrap_or_else(|_| "default".into());
        Self { base, workspace }
    }

    /// Create with explicit base path and workspace name.
    #[must_use]
    pub fn new(base: impl Into<PathBuf>, workspace: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            workspace: workspace.into(),
        }
    }

    /// Workspace root: `{base}/workspaces/{workspace}`
    #[must_use]
    pub fn root(&self) -> PathBuf {
        self.base.join("workspaces").join(&self.workspace)
    }

    /// Parquet store directory.
    #[must_use]
    pub fn store(&self) -> PathBuf {
        env_path_or("BRIGHTFLOW_STORE", || self.root().join("store"))
    }

    /// Litehouse (store metadata) SQLite URL.
    #[must_use]
    pub fn litehouse_url(&self) -> String {
        env_url_or("BRIGHTFLOW_LITEHOUSE_URL", || {
            self.root().join("litehouse.db")
        })
    }

    /// Auth SQLite URL.
    #[must_use]
    pub fn auth_url(&self) -> String {
        env_url_or("BRIGHTFLOW_DATABASE_URL", || self.root().join("auth.db"))
    }

    /// Scheduler SQLite URL.
    #[must_use]
    pub fn scheduler_url(&self) -> String {
        env_url_or("BRIGHTFLOW_SCHEDULER_DATABASE_URL", || {
            self.root().join("scheduler.db")
        })
    }

    /// Schema config directory.
    #[must_use]
    pub fn schemas(&self) -> PathBuf {
        env_path_or("BRIGHTFLOW_SCHEMA_DIR", || self.root().join("schemas"))
    }

    /// Connector config directory.
    #[must_use]
    pub fn connector_configs(&self) -> PathBuf {
        env_path_or("BRIGHTFLOW_CONNECTOR_CONFIGS", || {
            self.root().join("connector-configs")
        })
    }

    /// Event Parquet storage directory (per-source/date).
    #[must_use]
    pub fn events_store(&self) -> PathBuf {
        env_path_or("BRIGHTFLOW_EVENTS_STORE", || self.root().join("events"))
    }

    /// Event buffer directory (per-source SQLite DBs).
    #[must_use]
    pub fn events_buffer(&self) -> PathBuf {
        env_path_or("BRIGHTFLOW_EVENTS_BUFFER", || {
            self.root().join("events-buffer")
        })
    }

    /// Ingest metadata SQLite URL (sources, salts).
    #[must_use]
    pub fn ingest_url(&self) -> String {
        env_url_or("BRIGHTFLOW_INGEST_URL", || self.root().join("ingest.db"))
    }

    /// Base data directory.
    #[must_use]
    pub fn base(&self) -> &Path {
        &self.base
    }

    /// Create all workspace directories and parent dirs for DB files.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.store())?;
        std::fs::create_dir_all(self.schemas())?;
        std::fs::create_dir_all(self.connector_configs())?;
        std::fs::create_dir_all(self.events_store())?;
        std::fs::create_dir_all(self.events_buffer())?;
        // Ensure parent dirs for DB files exist
        for url in [
            self.litehouse_url(),
            self.auth_url(),
            self.scheduler_url(),
            self.ingest_url(),
        ] {
            if let Some(path) = url.strip_prefix("sqlite:") {
                let db_path = path.split('?').next().unwrap_or(path);
                if let Some(parent) = Path::new(db_path).parent() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }
        Ok(())
    }
}

/// Check env var for a path override, otherwise use the default.
fn env_path_or(var: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
    std::env::var(var).map_or_else(|_| default(), PathBuf::from)
}

/// Check env var for a SQLite URL override, otherwise build one from a path.
fn env_url_or(var: &str, default_path: impl FnOnce() -> PathBuf) -> String {
    std::env::var(var).unwrap_or_else(|_| format!("sqlite:{}?mode=rwc", default_path().display()))
}

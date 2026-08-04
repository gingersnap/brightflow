//! Workspace paths and cross-crate naming shared by every Brightflow crate.
//!
//! Kept tiny and dependency-free because nearly every crate depends on it —
//! anything added here lands in nearly every compile.
//!
//! `WorkspacePaths` is the single source of truth for where *data* lives: every
//! location derives from one base and one workspace name, which is what makes a
//! second workspace — or a throwaway test one — a matter of setting
//! `BRIGHTFLOW_DATA_DIR` and `BRIGHTFLOW_WORKSPACE` rather than auditing call
//! sites. Resolve data paths through here rather than joining your own.
//! `source_ids` plays the same role for how sources and event tables are
//! *named* in the store catalog.
//!
//! Scope is data locations and naming contracts. Asset paths — model
//! directories and similar — stay with the crate that owns the asset.

pub mod source_ids;

pub use source_ids::{connector_source_id, events_table_name, upload_source_id, web_source_id};

use std::path::{Path, PathBuf};

/// Resolved workspace paths — single source of truth for all data locations.
///
/// Each accessor resolves explicit builder override → env var →
/// `{base}/workspaces/{workspace}/{sub}` default.
#[derive(Debug, Clone)]
pub struct WorkspacePaths {
    base: PathBuf,
    workspace: String,
    /// Explicit overrides (builder methods) that beat both env vars and
    /// defaults — value-passing for CLI flags, so nothing needs to mutate
    /// process env inside a running runtime.
    store_override: Option<PathBuf>,
    connector_configs_override: Option<PathBuf>,
    auth_url_override: Option<String>,
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
        Self::new(base, workspace)
    }

    /// Create with explicit base path and workspace name.
    #[must_use]
    pub fn new(base: impl Into<PathBuf>, workspace: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            workspace: workspace.into(),
            store_override: None,
            connector_configs_override: None,
            auth_url_override: None,
        }
    }

    /// Explicitly override the store directory (beats `BRIGHTFLOW_STORE`).
    #[must_use]
    pub fn with_store(mut self, store: impl Into<PathBuf>) -> Self {
        self.store_override = Some(store.into());
        self
    }

    /// Explicitly override the connector-config directory (beats
    /// `BRIGHTFLOW_CONNECTOR_CONFIGS`).
    #[must_use]
    pub fn with_connector_configs(mut self, dir: impl Into<PathBuf>) -> Self {
        self.connector_configs_override = Some(dir.into());
        self
    }

    /// Explicitly override the auth SQLite URL (beats
    /// `BRIGHTFLOW_DATABASE_URL`).
    #[must_use]
    pub fn with_auth_url(mut self, url: impl Into<String>) -> Self {
        self.auth_url_override = Some(url.into());
        self
    }

    /// Workspace root: `{base}/workspaces/{workspace}`
    #[must_use]
    pub fn root(&self) -> PathBuf {
        self.base.join("workspaces").join(&self.workspace)
    }

    /// Parquet store directory. Precedence: builder override → env → default.
    #[must_use]
    pub fn store(&self) -> PathBuf {
        self.store_override
            .clone()
            .unwrap_or_else(|| env_path_or("BRIGHTFLOW_STORE", || self.root().join("store")))
    }

    /// Litehouse (store metadata) SQLite URL.
    #[must_use]
    pub fn litehouse_url(&self) -> String {
        env_url_or("BRIGHTFLOW_LITEHOUSE_URL", || {
            self.root().join("litehouse.db")
        })
    }

    /// Auth SQLite URL. Precedence: builder override → env → default.
    #[must_use]
    pub fn auth_url(&self) -> String {
        self.auth_url_override.clone().unwrap_or_else(|| {
            env_url_or("BRIGHTFLOW_DATABASE_URL", || self.root().join("auth.db"))
        })
    }

    /// Scheduler SQLite URL.
    #[must_use]
    pub fn scheduler_url(&self) -> String {
        env_url_or("BRIGHTFLOW_SCHEDULER_DATABASE_URL", || {
            self.root().join("scheduler.db")
        })
    }

    /// Connector output directory keyed by preset id.
    /// Two presets of the same connector type get isolated output directories.
    #[must_use]
    pub fn connector_output_for_preset(&self, preset_id: &str) -> PathBuf {
        self.root().join("connector-output").join(preset_id)
    }

    /// Connector config directory. Precedence: builder override → env →
    /// default.
    #[must_use]
    pub fn connector_configs(&self) -> PathBuf {
        self.connector_configs_override.clone().unwrap_or_else(|| {
            env_path_or("BRIGHTFLOW_CONNECTOR_CONFIGS", || {
                self.root().join("connector-configs")
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_overrides_beat_env_and_defaults() {
        let defaulted = WorkspacePaths::new("/base", "ws");
        assert_eq!(defaulted.store(), Path::new("/base/workspaces/ws/store"));

        // Override beats the default...
        let overridden = defaulted.with_store("/elsewhere/store");
        assert_eq!(overridden.store(), Path::new("/elsewhere/store"));

        // ...and beats the env var (this test owns BRIGHTFLOW_STORE; no other
        // test reads it, so the process-global write cannot race a reader).
        std::env::set_var("BRIGHTFLOW_STORE", "/from-env/store");
        assert_eq!(overridden.store(), Path::new("/elsewhere/store"));
        let plain = WorkspacePaths::new("/base", "ws");
        assert_eq!(
            plain.store(),
            Path::new("/from-env/store"),
            "env beats the default when no override is set"
        );
        std::env::remove_var("BRIGHTFLOW_STORE");
    }

    #[test]
    fn auth_url_and_connector_config_overrides() {
        let wp = WorkspacePaths::new("/base", "ws")
            .with_auth_url("sqlite:/tmp/x.db?mode=rwc")
            .with_connector_configs("/cfg");
        assert_eq!(wp.auth_url(), "sqlite:/tmp/x.db?mode=rwc");
        assert_eq!(wp.connector_configs(), Path::new("/cfg"));

        let plain = WorkspacePaths::new("/base", "ws");
        assert!(plain.auth_url().starts_with("sqlite:"));
        assert_eq!(
            plain.connector_configs(),
            Path::new("/base/workspaces/ws/connector-configs")
        );
    }
}

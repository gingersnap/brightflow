//! Manifest-based table storage format
//!
//! Each table is a directory containing:
//! - `manifest.json` — metadata + file list
//! - `data/` — parquet files

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::{StoreError, StoreResult};

/// Table manifest describing stored parquet files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: u64,
    pub schema: Option<serde_json::Value>,
    pub primary_keys: Vec<String>,
    pub files: Vec<FileEntry>,
    pub total_rows: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Entry for a single parquet file in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from table dir, e.g. "data/001_20250306T120000.parquet"
    pub path: String,
    pub num_rows: i64,
    pub size_bytes: u64,
    pub added_at: DateTime<Utc>,
}

const MANIFEST_FILE: &str = "manifest.json";
const DATA_DIR: &str = "data";

impl Manifest {
    /// Create an empty manifest for a new table
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            name: name.into(),
            version: 0,
            schema: None,
            primary_keys: Vec::new(),
            files: Vec::new(),
            total_rows: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Load a manifest from a table directory
    pub fn load(table_dir: &Path) -> StoreResult<Self> {
        let manifest_path = table_dir.join(MANIFEST_FILE);
        let content = std::fs::read_to_string(&manifest_path)?;
        serde_json::from_str(&content)
            .map_err(|e| StoreError::ManifestParse(format!("{}: {e}", manifest_path.display())))
    }

    /// Save the manifest atomically (write to .tmp, then rename)
    pub fn save(&self, table_dir: &Path) -> StoreResult<()> {
        let manifest_path = table_dir.join(MANIFEST_FILE);
        let tmp_path = table_dir.join(".manifest.json.tmp");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| StoreError::ManifestParse(e.to_string()))?;
        std::fs::write(&tmp_path, content)?;
        std::fs::rename(&tmp_path, &manifest_path)?;
        Ok(())
    }

    /// Add a file entry, bump version and update totals
    pub fn add_file(&mut self, entry: FileEntry) {
        self.total_rows += entry.num_rows;
        self.files.push(entry);
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Replace all files (used after merge rewrite)
    pub fn set_files(&mut self, entries: Vec<FileEntry>) {
        self.total_rows = entries.iter().map(|e| e.num_rows).sum();
        self.files = entries;
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Check if a manifest exists in the given table directory
    pub fn exists(table_dir: &Path) -> bool {
        table_dir.join(MANIFEST_FILE).exists()
    }

    /// Ensure the data subdirectory exists
    pub fn ensure_data_dir(table_dir: &Path) -> StoreResult<()> {
        std::fs::create_dir_all(table_dir.join(DATA_DIR))?;
        Ok(())
    }

    /// Generate a sequential filename for a new parquet file
    pub fn next_filename(&self) -> String {
        let seq = self.files.len() + 1;
        let ts = Utc::now().format("%Y%m%dT%H%M%S");
        format!("{DATA_DIR}/{seq:03}_{ts}.parquet")
    }
}

/// If a table directory has `_delta_log/` but no `manifest.json`, migrate it
/// to the manifest format by scanning existing parquet files.
pub fn migrate_from_delta_if_needed(table_dir: &Path) -> StoreResult<()> {
    if Manifest::exists(table_dir) {
        return Ok(());
    }

    let delta_log = table_dir.join("_delta_log");
    if !delta_log.exists() || !delta_log.is_dir() {
        return Ok(());
    }

    info!(
        "Migrating Delta table at {} to manifest format",
        table_dir.display()
    );

    let table_name = table_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut manifest = Manifest::new(table_name);

    // Ensure data/ subdir exists
    Manifest::ensure_data_dir(table_dir)?;

    let data_dir = table_dir.join(DATA_DIR);

    // Find all parquet files in the table root (Delta stores them there)
    let mut parquet_files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(table_dir)? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "parquet") {
            parquet_files.push(path);
        }
    }

    // Move each parquet file into data/ and add to manifest
    for (i, src) in parquet_files.iter().enumerate() {
        let filename = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.parquet");
        let dest_relative = format!("{DATA_DIR}/{:03}_{filename}", i + 1);
        let dest = table_dir.join(&dest_relative);

        std::fs::rename(src, &dest)?;

        let metadata = std::fs::metadata(&dest)?;
        let num_rows = count_parquet_rows(&dest);

        manifest.files.push(FileEntry {
            path: dest_relative,
            num_rows,
            size_bytes: metadata.len(),
            added_at: Utc::now(),
        });
        manifest.total_rows += num_rows;
    }

    // Also check if any parquet files are already in data/
    if data_dir.exists() {
        for entry in std::fs::read_dir(&data_dir)? {
            let path = entry?.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "parquet") {
                let rel = format!(
                    "{DATA_DIR}/{}",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown.parquet")
                );
                // Skip if already added via move
                if manifest.files.iter().any(|f| f.path == rel) {
                    continue;
                }
                let metadata = std::fs::metadata(&path)?;
                let num_rows = count_parquet_rows(&path);
                manifest.files.push(FileEntry {
                    path: rel,
                    num_rows,
                    size_bytes: metadata.len(),
                    added_at: Utc::now(),
                });
                manifest.total_rows += num_rows;
            }
        }
    }

    manifest.version = 1;
    manifest.save(table_dir)?;

    // Clean up _delta_log
    std::fs::remove_dir_all(&delta_log)?;

    info!(
        "Migrated Delta table: {} files, {} rows",
        manifest.files.len(),
        manifest.total_rows
    );

    Ok(())
}

/// Count rows in a parquet file using Polars ParquetReader
fn count_parquet_rows(path: &Path) -> i64 {
    use polars::prelude::{ParquetReader, SerReader};
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let Ok(df) = ParquetReader::new(file).finish() else {
        return 0;
    };
    i64::try_from(df.height()).unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_manifest_new() {
        let m = Manifest::new("test_table");
        assert_eq!(m.name, "test_table");
        assert_eq!(m.version, 0);
        assert!(m.files.is_empty());
        assert_eq!(m.total_rows, 0);
    }

    #[test]
    fn test_manifest_save_and_load() {
        let tmp = TempDir::new().expect("temp dir");
        let mut m = Manifest::new("test");
        m.add_file(FileEntry {
            path: "data/001.parquet".to_string(),
            num_rows: 100,
            size_bytes: 1024,
            added_at: Utc::now(),
        });
        m.save(tmp.path()).expect("save");

        let loaded = Manifest::load(tmp.path()).expect("load");
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.total_rows, 100);
    }
}

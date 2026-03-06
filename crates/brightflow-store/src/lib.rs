//! Brightflow Store - Manifest-based Parquet storage
//!
//! This crate provides Brightflow's lakehouse implementation using
//! a JSON manifest + Parquet files (replacing Delta Lake).

// Allow certain lints for store code:
// - similar_names: entry/dir_entry patterns are common in fs code
// - shadow_unrelated: variable shadowing for Result unwrapping is idiomatic
// - wildcard_imports: prelude imports are standard for polars
// - unused_async: async kept for API compatibility
#![allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::indexing_slicing,
    clippy::cast_sign_loss,
    clippy::match_same_arms,
    clippy::clone_on_ref_ptr,
    clippy::shadow_unrelated,
    clippy::shadow_reuse,
    clippy::wildcard_imports,
    clippy::unused_async
)]

mod error;
mod ingest;
mod manifest;
mod table;

pub use brightflow_core::{DatasetId, DatasetMeta, StorageConfig, TenantId};
pub use error::{StoreError, StoreResult};
pub use ingest::{IngestMode, IngestOptions, MergeMetrics};
pub use table::{TableInfo, TableRef};

use std::path::{Path, PathBuf};

use polars::prelude::*;
use tracing::info;

/// Parquet store for managing tables
pub struct ParquetStore {
    /// Root path for all tables
    root_path: PathBuf,
}

impl ParquetStore {
    /// Create a new `ParquetStore` with the given root path
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
        }
    }

    /// Create a `ParquetStore` from a `StorageConfig`
    pub fn from_config(config: &StorageConfig) -> StoreResult<Self> {
        match config {
            StorageConfig::Local { path } => Ok(Self::new(path)),
        }
    }

    /// Get the root path of the store
    #[must_use]
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get the path for a specific table
    fn table_path(&self, name: &str) -> PathBuf {
        self.root_path.join(name)
    }

    /// List all tables in the store
    pub async fn list_tables(&self) -> StoreResult<Vec<TableRef>> {
        table::list_tables(&self.root_path).await
    }

    /// Check if a table exists
    pub async fn table_exists(&self, name: &str) -> StoreResult<bool> {
        let path = self.table_path(name);
        table::table_exists(&path).await
    }

    /// Get information about a table
    pub async fn table_info(&self, name: &str) -> StoreResult<TableInfo> {
        let path = self.table_path(name);
        table::get_table_info(name, &path).await
    }

    /// Read a table as a Polars DataFrame
    pub async fn read_table(&self, name: &str) -> StoreResult<DataFrame> {
        let path = self.table_path(name);
        info!("Reading table '{}' from {:?}", name, path);
        table::read_table(&path).await
    }

    /// Get the parquet file paths for a table (for lazy scan mode)
    pub async fn get_table_parquet_paths(&self, name: &str) -> StoreResult<Vec<PathBuf>> {
        let path = self.table_path(name);
        if !table::table_exists(&path).await? {
            return Err(StoreError::TableNotFound(name.to_string()));
        }
        table::get_parquet_paths(&path)
    }

    /// Ingest a Parquet file into a table
    ///
    /// If the table doesn't exist, it will be created with the schema from the Parquet file.
    /// If it exists, data will be appended.
    pub async fn ingest_parquet(
        &self,
        table_name: &str,
        parquet_path: impl AsRef<Path>,
        options: Option<IngestOptions>,
    ) -> StoreResult<TableInfo> {
        let table_path = self.table_path(table_name);
        let ingest_options = options.unwrap_or_default();

        info!(
            "Ingesting parquet {:?} into table '{}' at {:?}",
            parquet_path.as_ref(),
            table_name,
            table_path
        );

        ingest::ingest_parquet(&table_path, parquet_path.as_ref(), &ingest_options).await?;
        self.table_info(table_name).await
    }

    /// Merge (upsert) a Parquet file into a table by primary key.
    /// Uses Polars join operations to update existing rows and insert new ones.
    pub async fn merge_parquet(
        &self,
        table_name: &str,
        parquet_path: impl AsRef<Path>,
        primary_keys: &[String],
    ) -> StoreResult<MergeMetrics> {
        let table_path = self.table_path(table_name);
        info!(
            "Merging parquet {:?} into table '{}' with PKs {:?}",
            parquet_path.as_ref(),
            table_name,
            primary_keys
        );
        ingest::merge_parquet(&table_path, parquet_path.as_ref(), primary_keys).await
    }

    /// Delete a table
    pub async fn delete_table(&self, name: &str) -> StoreResult<()> {
        let path = self.table_path(name);
        info!("Deleting table '{}' at {:?}", name, path);
        table::delete_table(&path).await
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_store() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let store = ParquetStore::new(tmp.path());
        assert_eq!(store.root_path(), tmp.path());
    }

    #[tokio::test]
    async fn test_list_empty_store() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let store = ParquetStore::new(tmp.path());
        let tables = store.list_tables().await.expect("failed to list tables");
        assert!(tables.is_empty());
    }
}

//! Brightflow Store - SQLite-backed Parquet storage (Litehouse)
//!
//! This crate provides Brightflow's lakehouse implementation using
//! SQLite metadata + Parquet data files.

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

pub mod db;
mod error;
mod ingest;
mod migrate;
mod models;
mod stats;
mod table;

pub use brightflow_core::{DatasetId, DatasetMeta, StorageConfig, TenantId};
pub use error::{StoreError, StoreResult};
pub use ingest::{IngestMode, IngestOptions, MergeMetrics};
pub use table::{TableInfo, TableRef};

use std::path::{Path, PathBuf};

use db::StoreDb;
use polars::prelude::*;
use tracing::info;

/// Parquet store for managing tables
pub struct ParquetStore {
    /// Root path for all tables
    root_path: PathBuf,
    /// SQLite metadata database
    db: StoreDb,
}

impl ParquetStore {
    /// Create a new `ParquetStore` with the given root path and database URL
    pub async fn new(root_path: impl Into<PathBuf>, database_url: &str) -> StoreResult<Self> {
        let db = StoreDb::new(database_url).await?;
        let root = root_path.into();

        // One-time migration from legacy manifest.json files
        migrate::migrate_manifests(&db, &root).await?;

        Ok(Self {
            root_path: root,
            db,
        })
    }

    /// Create a `ParquetStore` from a `StorageConfig` and database URL
    pub async fn from_config(config: &StorageConfig, database_url: &str) -> StoreResult<Self> {
        match config {
            StorageConfig::Local { path } => Self::new(path, database_url).await,
        }
    }

    /// Get the root path of the store
    #[must_use]
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// List all tables in the store
    pub async fn list_tables(&self) -> StoreResult<Vec<TableRef>> {
        table::list_tables(&self.db, &self.root_path).await
    }

    /// Check if a table exists
    pub async fn table_exists(&self, name: &str) -> StoreResult<bool> {
        table::table_exists(&self.db, name).await
    }

    /// Get information about a table
    pub async fn table_info(&self, name: &str) -> StoreResult<TableInfo> {
        table::get_table_info(&self.db, name, &self.root_path).await
    }

    /// Read a table as a Polars DataFrame
    pub async fn read_table(&self, name: &str) -> StoreResult<DataFrame> {
        info!("Reading table '{}'", name);
        table::read_table(&self.db, name, &self.root_path).await
    }

    /// Get the parquet file paths for a table (for lazy scan mode)
    pub async fn get_table_parquet_paths(&self, name: &str) -> StoreResult<Vec<PathBuf>> {
        table::get_parquet_paths(&self.db, name, &self.root_path).await
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
        let ingest_options = options.unwrap_or_default();

        info!(
            "Ingesting parquet {:?} into table '{}'",
            parquet_path.as_ref(),
            table_name,
        );

        ingest::ingest_parquet(
            &self.db,
            &self.root_path,
            table_name,
            parquet_path.as_ref(),
            &ingest_options,
        )
        .await?;
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
        info!(
            "Merging parquet {:?} into table '{}' with PKs {:?}",
            parquet_path.as_ref(),
            table_name,
            primary_keys
        );
        ingest::merge_parquet(
            &self.db,
            &self.root_path,
            table_name,
            parquet_path.as_ref(),
            primary_keys,
        )
        .await
    }

    /// Delete a table
    pub async fn delete_table(&self, name: &str) -> StoreResult<()> {
        info!("Deleting table '{}'", name);
        table::delete_table(&self.db, name, &self.root_path).await
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
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        let store = ParquetStore::new(tmp.path(), &db_url)
            .await
            .expect("failed to create store");
        assert_eq!(store.root_path(), tmp.path());
    }

    #[tokio::test]
    async fn test_list_empty_store() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let db_url = format!(
            "sqlite:{}?mode=rwc",
            tmp.path().join("litehouse.db").display()
        );
        let store = ParquetStore::new(tmp.path(), &db_url)
            .await
            .expect("failed to create store");
        let tables = store.list_tables().await.expect("failed to list tables");
        assert!(tables.is_empty());
    }
}

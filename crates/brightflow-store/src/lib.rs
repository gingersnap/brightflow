//! Brightflow Store - Delta Lake storage abstraction
//!
//! This crate provides Brightflow's lakehouse implementation using Delta Lake.

// Allow certain lints for store code:
// - similar_names: entry/dir_entry patterns are common in fs code
// - arc_with_non_send_sync: follows upstream delta-rs patterns
// - shadow_unrelated: variable shadowing for Result unwrapping is idiomatic
// - wildcard_imports: prelude imports are standard for polars
// - unused_async: async kept for future compatibility with async file I/O
#![allow(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::indexing_slicing,
    clippy::cast_sign_loss,
    clippy::match_same_arms,
    clippy::arc_with_non_send_sync,
    clippy::clone_on_ref_ptr,
    clippy::shadow_unrelated,
    clippy::wildcard_imports,
    clippy::unused_async
)]

mod error;
mod ingest;
mod table;

pub use brightflow_core::{DatasetId, DatasetMeta, StorageConfig, TenantId};
pub use error::{StoreError, StoreResult};
pub use ingest::{IngestMode, IngestOptions};
pub use table::{TableInfo, TableRef};

use std::path::{Path, PathBuf};

use deltalake::DeltaTable;
use polars::prelude::*;
use tracing::info;

/// Delta Lake store for managing tables
pub struct DeltaStore {
    /// Root path for all tables
    root_path: PathBuf,
}

impl DeltaStore {
    /// Create a new `DeltaStore` with the given root path
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
        }
    }

    /// Create a `DeltaStore` from a `StorageConfig`
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

    /// Read a specific version of a table as a Polars DataFrame
    pub async fn read_table_version(&self, name: &str, version: i64) -> StoreResult<DataFrame> {
        let path = self.table_path(name);
        info!(
            "Reading table '{}' version {} from {:?}",
            name, version, path
        );
        table::read_table_version(&path, version).await
    }

    /// Open a Delta table (low-level access)
    pub async fn open_table(&self, name: &str) -> StoreResult<DeltaTable> {
        let path = self.table_path(name);
        table::open_table(&path).await
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

    /// Ingest a Polars DataFrame into a table
    ///
    /// If the table doesn't exist, it will be created with the DataFrame's schema.
    /// If it exists, data will be appended.
    pub async fn ingest_dataframe(
        &self,
        table_name: &str,
        df: DataFrame,
        options: Option<IngestOptions>,
    ) -> StoreResult<TableInfo> {
        let table_path = self.table_path(table_name);
        let ingest_options = options.unwrap_or_default();

        info!(
            "Ingesting DataFrame ({} rows) into table '{}' at {:?}",
            df.height(),
            table_name,
            table_path
        );

        ingest::ingest_dataframe(&table_path, df, &ingest_options).await?;
        self.table_info(table_name).await
    }

    /// Get the parquet file paths for a table (for lazy scan mode)
    pub async fn get_table_parquet_paths(&self, name: &str) -> StoreResult<Vec<PathBuf>> {
        let path = self.table_path(name);
        if !table::table_exists(&path).await? {
            return Err(StoreError::TableNotFound(name.to_string()));
        }
        table::get_parquet_paths(&path)
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
        let store = DeltaStore::new(tmp.path());
        assert_eq!(store.root_path(), tmp.path());
    }

    #[tokio::test]
    async fn test_list_empty_store() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let store = DeltaStore::new(tmp.path());
        let tables = store.list_tables().await.expect("failed to list tables");
        assert!(tables.is_empty());
    }
}

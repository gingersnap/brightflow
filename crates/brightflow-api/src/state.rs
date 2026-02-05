use crate::analytics::session::{DatasetManager, DatasetSource};
use crate::shared::AppResult;
use brightflow_store::DeltaStore;
use polars::prelude::*;
use std::path::Path;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe storage for loaded datasets
    pub datasets: DatasetManager,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Create new AppState with empty dataset manager
    pub fn new() -> Self {
        Self {
            datasets: DatasetManager::new(),
        }
    }

    /// Create AppState and load a default dataset from CSV
    pub async fn with_default_dataset(csv_path: &str) -> AppResult<Self> {
        let state = Self::new();

        let path = csv_path.to_string();
        let df = tokio::task::spawn_blocking(move || -> Result<DataFrame, PolarsError> {
            CsvReadOptions::default()
                .with_infer_schema_length(Some(10000))
                .try_into_reader_with_file_path(Some(Path::new(&path).into()))?
                .finish()
        })
        .await??;

        let name = Path::new(csv_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("default")
            .to_string();

        state.datasets.add_dataset(name, df, DatasetSource::Default);

        Ok(state)
    }

    /// Load a Delta Lake table as a dataset
    ///
    /// # Arguments
    /// * `store` - The Delta store to read from
    /// * `table_name` - Name of the Delta table
    /// * `version` - Optional version (-1 or None for latest)
    ///
    /// # Returns
    /// The dataset ID that was assigned
    pub async fn load_delta_table(
        &self,
        store: &DeltaStore,
        table_name: &str,
        version: Option<i64>,
    ) -> AppResult<String> {
        let df = match version {
            Some(v) if v >= 0 => store.read_table_version(table_name, v).await?,
            _ => store.read_table(table_name).await?,
        };

        let version = version.unwrap_or(-1);
        let source = DatasetSource::DeltaTable {
            table_name: table_name.to_string(),
            version,
        };

        let id = self
            .datasets
            .add_dataset(table_name.to_string(), df, source);
        Ok(id)
    }

    /// Load all Delta Lake tables from a store
    ///
    /// # Returns
    /// Vector of (table_name, dataset_id) pairs
    pub async fn load_all_delta_tables(
        &self,
        store: &DeltaStore,
    ) -> Vec<(String, Result<String, String>)> {
        let tables = match store.list_tables().await {
            Ok(t) => t,
            Err(e) => return vec![("*".to_string(), Err(e.to_string()))],
        };

        let mut results = Vec::with_capacity(tables.len());
        for table_ref in tables {
            let result = match self.load_delta_table(store, &table_ref.name, None).await {
                Ok(id) => Ok(id),
                Err(e) => Err(e.to_string()),
            };
            results.push((table_ref.name, result));
        }
        results
    }
}

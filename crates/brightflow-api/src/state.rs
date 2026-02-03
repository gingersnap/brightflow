use crate::analytics::session::{DatasetManager, DatasetSource};
use crate::shared::AppResult;
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

        state
            .datasets
            .add_dataset(name, df, DatasetSource::Default);

        Ok(state)
    }
}

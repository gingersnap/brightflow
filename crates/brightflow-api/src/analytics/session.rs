use chrono::{DateTime, Utc};
use dashmap::DashMap;
use polars::prelude::*;
use serde::Serialize;
use std::sync::Arc;

/// Source of a dataset
#[derive(Clone, Debug)]
pub enum DatasetSource {
    /// Built-in default dataset
    Default,
    /// User-uploaded CSV file
    Upload { filename: String },
    /// Loaded from Delta Lake table
    DeltaTable {
        /// Name of the Delta table
        table_name: String,
        /// Version of the table (-1 for latest)
        version: i64,
    },
}

/// A loaded dataset with metadata
pub struct Dataset {
    pub name: String,
    pub df: DataFrame,
    pub loaded_at: DateTime<Utc>,
    pub source: DatasetSource,
}

impl Dataset {
    pub fn new(name: String, df: DataFrame, source: DatasetSource) -> Self {
        Self {
            name,
            df,
            loaded_at: Utc::now(),
            source,
        }
    }

    pub fn row_count(&self) -> usize {
        self.df.height()
    }

    pub fn column_count(&self) -> usize {
        self.df.width()
    }

    pub fn columns(&self) -> Vec<ColumnInfo> {
        self.df
            .get_columns()
            .iter()
            .map(|col| ColumnInfo {
                name: col.name().to_string(),
                dtype: dtype_to_string(col.dtype()),
            })
            .collect()
    }
}

/// Column metadata
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub dtype: String,
}

/// Summary info for a dataset
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetInfo {
    pub id: String,
    pub name: String,
    pub row_count: usize,
    pub column_count: usize,
    pub loaded_at: DateTime<Utc>,
}

/// Thread-safe manager for in-memory datasets
#[derive(Clone)]
pub struct DatasetManager {
    datasets: Arc<DashMap<String, Dataset>>,
}

impl Default for DatasetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DatasetManager {
    pub fn new() -> Self {
        Self {
            datasets: Arc::new(DashMap::new()),
        }
    }

    /// Add a new dataset, returns its ID
    pub fn add_dataset(&self, name: String, df: DataFrame, source: DatasetSource) -> String {
        let id = match &source {
            DatasetSource::Default => "default".to_string(),
            DatasetSource::Upload { .. } => uuid::Uuid::new_v4().to_string(),
            DatasetSource::DeltaTable { table_name, .. } => format!("delta:{table_name}"),
        };

        self.datasets
            .insert(id.clone(), Dataset::new(name, df, source));
        id
    }

    /// Get a reference to a dataset by ID
    pub fn get_dataset(&self, id: &str) -> Option<dashmap::mapref::one::Ref<'_, String, Dataset>> {
        self.datasets.get(id)
    }

    /// Delete a dataset by ID (cannot delete default)
    pub fn delete_dataset(&self, id: &str) -> bool {
        if id == "default" {
            return false;
        }
        self.datasets.remove(id).is_some()
    }

    /// List all datasets
    pub fn list_datasets(&self) -> Vec<DatasetInfo> {
        self.datasets
            .iter()
            .map(|entry| DatasetInfo {
                id: entry.key().clone(),
                name: entry.value().name.clone(),
                row_count: entry.value().row_count(),
                column_count: entry.value().column_count(),
                loaded_at: entry.value().loaded_at,
            })
            .collect()
    }

    /// Check if a dataset exists
    pub fn has_dataset(&self, id: &str) -> bool {
        self.datasets.contains_key(id)
    }
}

/// Convert Polars DataType to a display string
fn dtype_to_string(dtype: &DataType) -> String {
    match dtype {
        DataType::Boolean => "bool",
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => "int",
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => "uint",
        DataType::Float32 | DataType::Float64 => "float",
        DataType::String => "string",
        DataType::Datetime(_, _) => "datetime",
        DataType::Date => "date",
        DataType::Time => "time",
        DataType::Duration(_) => "duration",
        DataType::Null => "null",
        DataType::List(_) => "list",
        DataType::Struct(_) => "struct",
        _ => "unknown",
    }
    .to_string()
}

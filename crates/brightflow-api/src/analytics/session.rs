use std::path::PathBuf;
use std::sync::Arc;

use brightflow_core::DataMode;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use polars::prelude::*;
use serde::Serialize;

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

/// The underlying data for a dataset — either eagerly loaded or lazy scan references
#[derive(Clone, Debug)]
pub enum DatasetData {
    /// Fully materialized in-memory DataFrame
    Eager(DataFrame),
    /// References to parquet files, scanned lazily per query
    Lazy { parquet_files: Vec<PathBuf> },
}

/// A loaded dataset with metadata
pub struct Dataset {
    pub name: String,
    pub data: DatasetData,
    pub data_mode: DataMode,
    pub loaded_at: DateTime<Utc>,
    pub source: DatasetSource,
}

impl Dataset {
    pub fn new(
        name: String,
        data: DatasetData,
        data_mode: DataMode,
        source: DatasetSource,
    ) -> Self {
        Self {
            name,
            data,
            data_mode,
            loaded_at: Utc::now(),
            source,
        }
    }

    pub fn row_count(&self) -> Option<usize> {
        match &self.data {
            DatasetData::Eager(df) => Some(df.height()),
            DatasetData::Lazy { .. } => None,
        }
    }

    pub fn column_count(&self) -> Option<usize> {
        match &self.data {
            DatasetData::Eager(df) => Some(df.width()),
            DatasetData::Lazy { .. } => None,
        }
    }

    pub fn columns(&self) -> Vec<ColumnInfo> {
        match &self.data {
            DatasetData::Eager(df) => df
                .get_columns()
                .iter()
                .map(|col| ColumnInfo {
                    name: col.name().to_string(),
                    dtype: dtype_to_string(col.dtype()),
                })
                .collect(),
            DatasetData::Lazy { parquet_files } => {
                // Read schema from the first parquet file
                if let Some(first) = parquet_files.first() {
                    if let Ok(mut lf) = LazyFrame::scan_parquet(first, ScanArgsParquet::default()) {
                        if let Ok(schema) = lf.collect_schema() {
                            return schema
                                .iter()
                                .map(|(name, dtype)| ColumnInfo {
                                    name: name.to_string(),
                                    dtype: dtype_to_string(dtype),
                                })
                                .collect();
                        }
                    }
                }
                Vec::new()
            },
        }
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
    pub row_count: Option<usize>,
    pub column_count: Option<usize>,
    pub data_mode: String,
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
    pub fn add_dataset(
        &self,
        name: String,
        data: DatasetData,
        data_mode: DataMode,
        source: DatasetSource,
    ) -> String {
        let id = match &source {
            DatasetSource::Default => "default".to_string(),
            DatasetSource::Upload { .. } => uuid::Uuid::new_v4().to_string(),
            DatasetSource::DeltaTable { table_name, .. } => format!("delta:{table_name}"),
        };

        self.datasets
            .insert(id.clone(), Dataset::new(name, data, data_mode, source));
        id
    }

    /// Get a clone of the dataset data for use in blocking tasks
    pub fn get_data(&self, id: &str) -> Option<DatasetData> {
        self.datasets.get(id).map(|d| d.data.clone())
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
                data_mode: entry.value().data_mode.to_string(),
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

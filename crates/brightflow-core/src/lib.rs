use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Unique identifier for a tenant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub Uuid);

impl TenantId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a dataset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatasetId(pub Uuid);

impl DatasetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DatasetId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DatasetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Metadata about a dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMeta {
    pub id: DatasetId,
    pub tenant_id: TenantId,
    pub name: String,
    pub source: String,
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DatasetMeta {
    pub fn new(tenant_id: TenantId, name: String, source: String) -> Self {
        let now = Utc::now();
        Self {
            id: DatasetId::new(),
            tenant_id,
            name,
            source,
            schema_version: 1,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Storage configuration for data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StorageConfig {
    Local {
        path: String,
    },
    S3 {
        endpoint: Option<String>,
        bucket: String,
        region: String,
    },
}

/// Core error type for Brightflow
#[derive(Debug, Error)]
pub enum BrightflowError {
    #[error("Dataset not found: {0}")]
    DatasetNotFound(DatasetId),

    #[error("Tenant not found: {0}")]
    TenantNotFound(TenantId),

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

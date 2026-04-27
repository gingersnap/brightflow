use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::nlp::FittedTfIdf;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("artifact io error at {path}: {message}")]
    Io { path: String, message: String },
    #[error("artifact serialization error: {0}")]
    Serde(String),
}

impl ArtifactError {
    fn io(path: &Path, e: &std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        }
    }
}

/// TF-IDF vocabulary + IDF weights, used for top-term extraction (cluster naming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TfIdfArtifact {
    pub fitted: FittedTfIdf,
    pub embedding_model_id: String,
}

/// Dense k-means centroids in embedding space, plus human-readable cluster names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteringArtifact {
    pub centroids: Vec<Vec<f32>>,
    pub names: Vec<String>,
    /// Top distinctive terms per cluster (c-TF-IDF).
    #[serde(default)]
    pub top_terms: Vec<Vec<String>>,
    /// Three representative document titles per cluster.
    #[serde(default)]
    pub sample_titles: Vec<Vec<String>>,
    pub embedding_model_id: String,
    pub k: usize,
    pub fitted_at: i64,
}

/// Per-label centroid in embedding space; classify by nearest centroid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelCentroidsArtifact {
    pub centroids: HashMap<String, Vec<f32>>,
    pub embedding_model_id: String,
}

/// Cheap-to-read summary surfaced by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub embedding_model_id: String,
    pub k: usize,
    pub fitted_at: i64,
    pub total_rows: usize,
    pub has_labels: bool,
}

const TFIDF_FILE: &str = "tfidf.bin";
const CLUSTERS_FILE: &str = "clusters.bin";
const LABELS_FILE: &str = "labels.bin";
const META_FILE: &str = "meta.json";

fn save_bincode<T: Serialize>(value: &T, path: &Path) -> Result<(), ArtifactError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ArtifactError::io(parent, &e))?;
    }
    let bytes = bincode::serialize(value).map_err(|e| ArtifactError::Serde(e.to_string()))?;
    std::fs::write(path, bytes).map_err(|e| ArtifactError::io(path, &e))?;
    Ok(())
}

fn load_bincode<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ArtifactError> {
    let bytes = std::fs::read(path).map_err(|e| ArtifactError::io(path, &e))?;
    bincode::deserialize(&bytes).map_err(|e| ArtifactError::Serde(e.to_string()))
}

impl TfIdfArtifact {
    pub fn save(&self, dir: &Path) -> Result<(), ArtifactError> {
        save_bincode(self, &dir.join(TFIDF_FILE))
    }
    pub fn load(dir: &Path) -> Result<Self, ArtifactError> {
        load_bincode(&dir.join(TFIDF_FILE))
    }
    pub fn exists(dir: &Path) -> bool {
        dir.join(TFIDF_FILE).exists()
    }
}

impl ClusteringArtifact {
    pub fn save(&self, dir: &Path) -> Result<(), ArtifactError> {
        save_bincode(self, &dir.join(CLUSTERS_FILE))
    }
    pub fn load(dir: &Path) -> Result<Self, ArtifactError> {
        load_bincode(&dir.join(CLUSTERS_FILE))
    }
    pub fn exists(dir: &Path) -> bool {
        dir.join(CLUSTERS_FILE).exists()
    }
}

impl LabelCentroidsArtifact {
    pub fn save(&self, dir: &Path) -> Result<(), ArtifactError> {
        save_bincode(self, &dir.join(LABELS_FILE))
    }
    pub fn load(dir: &Path) -> Result<Self, ArtifactError> {
        load_bincode(&dir.join(LABELS_FILE))
    }
    pub fn exists(dir: &Path) -> bool {
        dir.join(LABELS_FILE).exists()
    }
}

impl ArtifactMeta {
    pub fn save(&self, dir: &Path) -> Result<(), ArtifactError> {
        let path = dir.join(META_FILE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ArtifactError::io(parent, &e))?;
        }
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| ArtifactError::Serde(e.to_string()))?;
        std::fs::write(&path, json).map_err(|e| ArtifactError::io(&path, &e))?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self, ArtifactError> {
        let path = dir.join(META_FILE);
        let bytes = std::fs::read(&path).map_err(|e| ArtifactError::io(&path, &e))?;
        serde_json::from_slice(&bytes).map_err(|e| ArtifactError::Serde(e.to_string()))
    }

    pub fn exists(dir: &Path) -> bool {
        dir.join(META_FILE).exists()
    }
}

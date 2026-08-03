//! Persisted fitted models: TF-IDF, clustering, label centroids, classifiers.
//!
//! Every artifact carries `ARTIFACT_VERSION`, and a stale or undecodable one is
//! treated as "refit needed" rather than as an error — see `ARTIFACT_VERSION`
//! for that contract. What matters is that it is never *used*: an artifact whose
//! layout has shifted would otherwise deserialize into plausible numbers and
//! produce confidently wrong labels. Refitting is cheap; wrong labels are not.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::nlp::{FittedTfIdf, MultiLabelLinear};

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

/// Current on-disk format version for fitted artifacts.
///
/// Bump when the binary layout of `ClusteringArtifact` changes; readers treat
/// version mismatches (and undecodable files) as "refit needed", not errors.
pub const ARTIFACT_VERSION: u32 = 3;

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
    /// Per-cluster minimum cosine similarity for assignment (from outlier trim).
    #[serde(default)]
    pub assign_thresholds: Vec<f32>,
    /// Embedding dimensionality the centroids were fitted in.
    #[serde(default)]
    pub dim: usize,
    /// Language this fit covers (primary subtag, e.g. "en"), when a language
    /// column was present.
    #[serde(default)]
    pub language: Option<String>,
    /// Distribution of languages seen at fit time: (language, row count).
    #[serde(default)]
    pub language_histogram: Vec<(String, usize)>,
    #[serde(default)]
    pub artifact_version: u32,
    /// Clustering algorithm used: "kmeans" | "hdbscan".
    #[serde(default)]
    pub algorithm: String,
    /// min_cluster_size used (hdbscan) — recorded for reproducibility.
    #[serde(default)]
    pub min_cluster_size: Option<usize>,
    /// PCA dims used before density clustering, when applicable.
    #[serde(default)]
    pub pca_dims: Option<usize>,
}

/// Per-label centroid in embedding space; classify by nearest centroid.
///
/// Superseded by [`ClassifierArtifact`] as the labeler, but **deliberately
/// retained**: it is the fallback that keeps artifact directories fitted before
/// the classifier existed working. Deleting it would orphan every old fit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelCentroidsArtifact {
    pub centroids: HashMap<String, Vec<f32>>,
    pub embedding_model_id: String,
}

/// Trained supervised head mapping an embedding to intent labels.
///
/// This is the artifact that breaks the format-cluster problem: unlike
/// [`LabelCentroidsArtifact`], whose nearest-centroid rule weights every
/// embedding dimension equally and therefore inherits the format bias, a
/// trained head learns to downweight format-correlated dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifierArtifact {
    pub head: MultiLabelLinear,
    /// Label names, parallel to `head.weights`.
    pub labels: Vec<String>,
    pub dim: usize,
    pub embedding_model_id: String,
    pub fitted_at: i64,
    #[serde(default)]
    pub artifact_version: u32,
    /// Held-out macro-F1 at fit time — the claim, checkable in production.
    #[serde(default)]
    pub val_macro_f1: f32,
    /// Positive-row count per label, parallel to `labels`.
    #[serde(default)]
    pub support: Vec<usize>,
    #[serde(default)]
    pub train_rows: usize,
}

impl ClassifierArtifact {
    /// Whether this head can be applied to embeddings from `model_id` at `dim`.
    ///
    /// Checks provenance (model, dim, version) **and internal consistency**. The
    /// consistency half is not paranoia: bincode will happily deserialize a
    /// truncated file into ragged vectors, and a head whose `weights` and
    /// `labels` have drifted out of alignment scores rows against the wrong
    /// label — silently, and forever, since nothing downstream re-checks.
    pub fn is_compatible(&self, model_id: &str, dim: usize) -> bool {
        self.embedding_model_id == model_id
            && self.dim == dim
            && self.artifact_version == ARTIFACT_VERSION
            && self.head.weights.len() == self.labels.len()
            && self.head.biases.len() == self.labels.len()
            && self.head.thresholds.len() == self.labels.len()
            && !self.labels.is_empty()
            && self.head.weights.iter().all(|w| w.len() == dim)
    }
}

/// Cheap-to-read summary surfaced by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub embedding_model_id: String,
    pub k: usize,
    pub fitted_at: i64,
    pub total_rows: usize,
    pub has_labels: bool,
    /// Language this fit covers (primary subtag), if a language column exists.
    #[serde(default)]
    pub language: Option<String>,
    /// Format version of the sibling binary artifacts (serde-default 0 for
    /// pre-versioning fits, which readers treat as stale).
    #[serde(default)]
    pub artifact_version: u32,
    /// Clustering algorithm used: "kmeans" | "hdbscan".
    #[serde(default)]
    pub algorithm: String,
    /// min_cluster_size used (hdbscan) — recorded for reproducibility.
    #[serde(default)]
    pub min_cluster_size: Option<usize>,
    /// PCA dims used before density clustering, when applicable.
    #[serde(default)]
    pub pca_dims: Option<usize>,
    /// Whether a trained classifier head sits beside this fit.
    #[serde(default)]
    pub has_classifier: bool,
    /// Held-out macro-F1 of that head, when one exists.
    #[serde(default)]
    pub classifier_val_macro_f1: Option<f32>,
}

const TFIDF_FILE: &str = "tfidf.bin";
const CLUSTERS_FILE: &str = "clusters.bin";
const LABELS_FILE: &str = "labels.bin";
const CLASSIFIER_FILE: &str = "classifier.bin";
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

impl ClassifierArtifact {
    pub fn save(&self, dir: &Path) -> Result<(), ArtifactError> {
        save_bincode(self, &dir.join(CLASSIFIER_FILE))
    }
    pub fn load(dir: &Path) -> Result<Self, ArtifactError> {
        load_bincode(&dir.join(CLASSIFIER_FILE))
    }
    pub fn exists(dir: &Path) -> bool {
        dir.join(CLASSIFIER_FILE).exists()
    }
    /// Remove a stale head. Used when a refit finds too little signal to train:
    /// leaving the previous head in place would score new rows against a model
    /// fitted to labels that no longer exist.
    pub fn remove(dir: &Path) -> Result<(), ArtifactError> {
        let path = dir.join(CLASSIFIER_FILE);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ArtifactError::io(&path, &e)),
        }
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

#[cfg(test)]
mod tests {
    use super::{ArtifactError, ClassifierArtifact, ARTIFACT_VERSION};
    use crate::nlp::MultiLabelLinear;

    fn head(dim: usize, n: usize) -> MultiLabelLinear {
        MultiLabelLinear {
            weights: vec![vec![0.1; dim]; n],
            biases: vec![0.0; n],
            thresholds: vec![0.5; n],
        }
    }

    fn artifact(dim: usize, n: usize) -> ClassifierArtifact {
        ClassifierArtifact {
            head: head(dim, n),
            labels: (0..n).map(|i| format!("label-{i}")).collect(),
            dim,
            embedding_model_id: "potion-base-32M".to_string(),
            fitted_at: 0,
            artifact_version: ARTIFACT_VERSION,
            val_macro_f1: 0.8,
            support: vec![100; n],
            train_rows: 500,
        }
    }

    #[test]
    fn compatible_when_provenance_and_shape_agree() {
        assert!(artifact(4, 3).is_compatible("potion-base-32M", 4));
    }

    #[test]
    fn incompatible_on_model_dim_or_version_mismatch() {
        let a = artifact(4, 3);
        assert!(!a.is_compatible("some-other-model", 4), "model must match");
        assert!(!a.is_compatible("potion-base-32M", 8), "dim must match");

        let mut stale = artifact(4, 3);
        stale.artifact_version = ARTIFACT_VERSION.wrapping_sub(1);
        assert!(
            !stale.is_compatible("potion-base-32M", 4),
            "version must match"
        );
    }

    #[test]
    fn incompatible_when_weights_and_labels_disagree() {
        // The truncated-bincode case: a head that scores 3 label-vectors but
        // only knows 2 names would silently report row intents under the wrong
        // label. It must be rejected, not used.
        let mut a = artifact(4, 3);
        a.labels.pop();
        assert!(!a.is_compatible("potion-base-32M", 4));
    }

    #[test]
    fn incompatible_when_a_weight_row_is_ragged() {
        let mut a = artifact(4, 3);
        a.head.weights[1] = vec![0.1; 2];
        assert!(!a.is_compatible("potion-base-32M", 4));
    }

    #[test]
    fn incompatible_when_biases_or_thresholds_are_short() {
        let mut a = artifact(4, 3);
        a.head.biases.pop();
        assert!(!a.is_compatible("potion-base-32M", 4));

        let mut b = artifact(4, 3);
        b.head.thresholds.pop();
        assert!(!b.is_compatible("potion-base-32M", 4));
    }

    #[test]
    fn incompatible_when_empty() {
        assert!(!artifact(4, 0).is_compatible("potion-base-32M", 4));
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("bf-classifier-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let a = artifact(4, 3);
        a.save(&dir).expect("save");
        assert!(ClassifierArtifact::exists(&dir));

        let loaded = ClassifierArtifact::load(&dir).expect("load");
        assert!(loaded.is_compatible("potion-base-32M", 4));
        assert_eq!(loaded.labels, a.labels);
        assert_eq!(loaded.head.weights, a.head.weights);
        assert_eq!(loaded.head.thresholds, a.head.thresholds);

        ClassifierArtifact::remove(&dir).expect("remove");
        assert!(!ClassifierArtifact::exists(&dir));
        // Removing a missing head is a no-op, not an error.
        ClassifierArtifact::remove(&dir).expect("remove again");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn truncated_file_fails_to_load_rather_than_scoring_wrong() {
        let dir = std::env::temp_dir().join(format!("bf-classifier-trunc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        artifact(4, 3).save(&dir).expect("save");

        let path = dir.join(super::CLASSIFIER_FILE);
        let bytes = std::fs::read(&path).expect("read");
        std::fs::write(&path, &bytes[..bytes.len() / 2]).expect("truncate");

        // Either it fails to decode, or it decodes into something is_compatible
        // rejects. Both are safe; silently scoring is not.
        match ClassifierArtifact::load(&dir) {
            Err(ArtifactError::Serde(_) | ArtifactError::Io { .. }) => {},
            Ok(a) => assert!(
                !a.is_compatible("potion-base-32M", 4),
                "a truncated head must never pass is_compatible"
            ),
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}

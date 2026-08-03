//! Loads and runs the static embedding model.
//!
//! The model is loaded once into a process-wide `OnceCell` — it is tens of
//! megabytes, so per-call loading would dominate every enrichment run. A missing
//! model file is a normal, recoverable state (embeddings are optional), so it
//! surfaces as an error the caller can report rather than a panic.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use model2vec_rs::model::StaticModel;
use once_cell::sync::OnceCell;
use thiserror::Error;
use tracing::info;

use super::paths::embedder_path;

#[derive(Debug, Error)]
pub enum EmbedderError {
    #[error("embedder model directory not found: {path}")]
    NotFound { path: PathBuf },
    #[error("embedder load failed at {path}: {message}")]
    LoadFailed { path: PathBuf, message: String },
    #[error("encoder returned {got} embeddings for {expected} inputs")]
    LengthMismatch { got: usize, expected: usize },
}

static EMBEDDER: OnceCell<Arc<StaticModel>> = OnceCell::new();

/// Return the shared embedder, loading it lazily on first call.
///
/// The model is ~120 MB; sharing one `Arc<StaticModel>` across the scheduler,
/// API, and CLI keeps RAM and startup cost flat.
pub fn shared_embedder(workspace_root: &Path) -> Result<Arc<StaticModel>, EmbedderError> {
    if let Some(existing) = EMBEDDER.get() {
        return Ok(Arc::clone(existing));
    }

    let path = embedder_path(workspace_root);
    if !path.exists() {
        return Err(EmbedderError::NotFound { path });
    }

    info!("Loading Model2Vec encoder from {}", path.display());
    let model = StaticModel::from_pretrained(&path, None, None, None).map_err(|e| {
        EmbedderError::LoadFailed {
            path: path.clone(),
            message: e.to_string(),
        }
    })?;

    let arc = Arc::new(model);
    // If another thread won the race, prefer the existing instance and drop ours.
    let stored = EMBEDDER.get_or_init(|| Arc::clone(&arc));
    Ok(Arc::clone(stored))
}

/// Encode a batch of strings to dense embeddings.
///
/// Returns one `Vec<f32>` per input. Output is L2-normalized by the model.
pub fn embed_batch(model: &StaticModel, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let embeddings = model.encode(texts);
    if embeddings.len() != texts.len() {
        return Err(EmbedderError::LengthMismatch {
            got: embeddings.len(),
            expected: texts.len(),
        });
    }
    Ok(embeddings)
}

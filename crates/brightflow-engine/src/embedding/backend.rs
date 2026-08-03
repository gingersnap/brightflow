//! Pluggable embedding backends.
//!
//! Model2Vec (potion) is the sole backend — a static token-averaging model
//! with ~ms latency. Semantic quality comes from over-clustering plus LLM
//! agent curation (labels, merges) rather than heavier embeddings. The
//! backend abstraction stays so a future backend is additive: switching
//! changes the effective model id, so cached row embeddings invalidate
//! lazily — nothing to migrate.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use super::encoder::{embed_batch, shared_embedder, EmbedderError};
use super::paths::POTION_BASE_32M_DIR;

/// A loaded embedding model.
pub trait EmbedderBackend: Send + Sync {
    /// Base model identifier (before the cleaning-profile suffix).
    fn model_id(&self) -> &str;
    /// Output dimensionality.
    fn dim(&self) -> usize;
    /// Encode a batch; one L2-normalized vector per input.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError>;
}

/// Selectable embedder, persisted in enrichment settings by its `name()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EmbedderId {
    /// Model2Vec potion-base-32M — static, fast, bundled.
    #[default]
    PotionBase32M,
}

impl EmbedderId {
    /// Stable name persisted in settings.
    pub fn name(self) -> &'static str {
        match self {
            Self::PotionBase32M => "potion-base-32M",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "potion-base-32M" => Some(Self::PotionBase32M),
            _ => None,
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::PotionBase32M]
    }

    /// Output dimensionality.
    pub fn dim(self) -> usize {
        match self {
            Self::PotionBase32M => 512,
        }
    }
}

type BackendRegistry = HashMap<EmbedderId, Arc<dyn EmbedderBackend>>;

/// Per-id backend registry (one loaded model per process).
static BACKENDS: LazyLock<Mutex<BackendRegistry>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Return (loading if needed) the backend for an id.
pub fn get_backend(
    workspace_root: &Path,
    id: EmbedderId,
) -> Result<Arc<dyn EmbedderBackend>, EmbedderError> {
    if let Ok(map) = BACKENDS.lock() {
        if let Some(existing) = map.get(&id) {
            return Ok(Arc::clone(existing));
        }
    }
    let backend: Arc<dyn EmbedderBackend> = match id {
        EmbedderId::PotionBase32M => Arc::new(PotionBackend {
            model: shared_embedder(workspace_root)?,
        }),
    };
    if let Ok(mut map) = BACKENDS.lock() {
        let entry = map.entry(id).or_insert_with(|| Arc::clone(&backend));
        return Ok(Arc::clone(entry));
    }
    Ok(backend)
}

// ─── Model2Vec backend ────────────────────────────────────────────────────────

struct PotionBackend {
    model: Arc<model2vec_rs::model::StaticModel>,
}

impl EmbedderBackend for PotionBackend {
    fn model_id(&self) -> &str {
        POTION_BASE_32M_DIR
    }

    fn dim(&self) -> usize {
        EmbedderId::PotionBase32M.dim()
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        embed_batch(&self.model, texts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_names_round_trip() {
        for id in EmbedderId::all() {
            assert_eq!(EmbedderId::parse(id.name()), Some(*id));
        }
        assert_eq!(EmbedderId::parse("bogus"), None);
        assert_eq!(EmbedderId::parse("fastembed:multilingual-e5-small"), None);
    }
}

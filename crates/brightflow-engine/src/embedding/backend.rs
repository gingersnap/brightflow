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

/// Indices and owned texts of the rows that survived cleaning: the inputs an
/// embed call wants (`Some` rows only), paired with where each row came from.
#[must_use]
pub fn eligible_texts(texts: &[Option<String>]) -> (Vec<usize>, Vec<String>) {
    let mut indices = Vec::new();
    let mut owned = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        if let Some(t) = t {
            indices.push(i);
            owned.push(t.clone());
        }
    }
    (indices, owned)
}

/// Embed the `Some` rows of an aligned text column, scattering vectors back.
///
/// The result is index-aligned with `texts` — and therefore with whatever
/// rows `texts` was built from; rows whose text is `None` stay `None`. The
/// alignment is the point: callers compare vectors against per-row data, and
/// an off-by-one here silently pairs every row with a neighbour's embedding.
pub fn embed_aligned(
    backend: &dyn EmbedderBackend,
    texts: &[Option<String>],
) -> Result<Vec<Option<Vec<f32>>>, EmbedderError> {
    let (indices, owned) = eligible_texts(texts);
    let vectors = backend.embed(&owned)?;
    let mut aligned: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    for (slot, vector) in indices.into_iter().zip(vectors) {
        if let Some(cell) = aligned.get_mut(slot) {
            *cell = Some(vector);
        }
    }
    Ok(aligned)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::cast_precision_loss,
        reason = "test double encodes tiny text lengths as f32"
    )]

    use super::*;

    /// Deterministic test double: "embeds" a text as `[len]`, so tests can
    /// tell exactly which text landed in which output slot.
    struct LenBackend;

    impl EmbedderBackend for LenBackend {
        fn model_id(&self) -> &'static str {
            "len-test"
        }
        fn dim(&self) -> usize {
            1
        }
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
            Ok(texts.iter().map(|t| vec![t.len() as f32]).collect())
        }
    }

    #[test]
    fn eligible_texts_keeps_indices_and_order() {
        let texts = vec![None, Some("ab".to_string()), None, Some("cdef".to_string())];
        let (indices, owned) = eligible_texts(&texts);
        assert_eq!(indices, vec![1, 3]);
        assert_eq!(owned, vec!["ab", "cdef"]);
        assert_eq!(eligible_texts(&[]), (Vec::new(), Vec::new()));
    }

    #[test]
    fn embed_aligned_scatters_back_to_source_rows() {
        let texts = vec![None, Some("ab".to_string()), None, Some("cdef".to_string())];
        let aligned = embed_aligned(&LenBackend, &texts).unwrap();
        assert_eq!(aligned.len(), texts.len());
        assert_eq!(aligned[0], None);
        assert_eq!(aligned[1], Some(vec![2.0]));
        assert_eq!(aligned[2], None);
        assert_eq!(aligned[3], Some(vec![4.0]));
    }

    #[test]
    fn embed_aligned_all_none_never_calls_into_trouble() {
        let texts = vec![None, None];
        let aligned = embed_aligned(&LenBackend, &texts).unwrap();
        assert_eq!(aligned, vec![None, None]);
    }

    #[test]
    fn id_names_round_trip() {
        for id in EmbedderId::all() {
            assert_eq!(EmbedderId::parse(id.name()), Some(*id));
        }
        assert_eq!(EmbedderId::parse("bogus"), None);
        assert_eq!(EmbedderId::parse("fastembed:multilingual-e5-small"), None);
    }
}

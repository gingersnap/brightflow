//! Dense sentence embeddings via a bundled Model2Vec encoder.

pub mod backend;
pub mod encoder;
pub mod paths;

pub use backend::{get_backend, EmbedderBackend, EmbedderId};
pub use encoder::{embed_batch, shared_embedder, EmbedderError};
pub use paths::{embedder_path, sanitize_source_id, topics_artifact_dir, POTION_BASE_32M_DIR};

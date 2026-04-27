pub mod encoder;
pub mod paths;

pub use encoder::{embed_batch, shared_embedder, EmbedderError, EMBEDDING_DIM};
pub use paths::{embedder_path, sanitize_source_id, topics_artifact_dir, POTION_BASE_32M_DIR};

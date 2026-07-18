//! Text Explorer: instant include/exclude word & phrase filtering.
//!
//! Serves a table's text rows with highlighted results and a
//! common/distinctive words widget. No LLM, no Polars expressions — plain
//! substring matching over an in-memory per-table index (see `index.rs`).

pub mod handlers;
pub(crate) mod highlight;
pub mod index;
pub(crate) mod score;
pub mod types;

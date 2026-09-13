//! Ticket enrichment: the two built-in LLM calls and their vocabularies.
//!
//! Classification and mention extraction, the vocabularies they resolve
//! against, and the function specs that snapshot them. Everything here is
//! pure; the async runner and the store live in the API crate.

pub mod function;
pub mod mentions;
pub mod ticket_classify;
pub mod vocabulary;

use crate::data::config::ColumnRole;
use brightflow_types::LogicalType;

/// What a materialised output column means.
///
/// Declared beside the column list that writes it so the two cannot drift
/// apart. The runner turns these into `column_semantics` rows the first time
/// a column lands; a user's later edit to the row wins over a re-run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputSemantic {
    pub name: &'static str,
    pub datatype: LogicalType,
    pub role: ColumnRole,
    pub label: &'static str,
    pub description: &'static str,
}

pub use function::{
    input_hash, ticket_classify_hash, ticket_extract_hash, FunctionSpec, TicketClassifySpec,
    TicketExtractSpec, VocabEntry,
};
pub use vocabulary::{
    check_cap, health, is_other, CapError, LevelHealth, VocabKind, HARD_BACKSTOP, IMPORTED_CAP,
    INDUCED_CAP, MIN_ROWS_PER_ENTRY, OTHER, OTHER_PARENT,
};

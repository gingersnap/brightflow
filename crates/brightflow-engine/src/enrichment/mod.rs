//! Ticket enrichment: the two built-in LLM calls and their vocabularies.
//!
//! Classification and mention extraction, the vocabularies they resolve
//! against, and the function specs that snapshot them. Everything here is
//! pure; the async runner and the store live in the API crate.

pub mod function;
pub mod mentions;
pub mod ticket_classify;
pub mod vocabulary;

pub use function::{
    input_hash, ticket_classify_hash, ticket_extract_hash, FunctionSpec, TicketClassifySpec,
    TicketExtractSpec, VocabEntry,
};
pub use vocabulary::{
    check_cap, health, is_other, CapError, LevelHealth, VocabKind, HARD_BACKSTOP, IMPORTED_CAP,
    INDUCED_CAP, OTHER,
};

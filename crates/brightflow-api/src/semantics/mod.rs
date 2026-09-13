//! The semantics surface: what a table's columns *mean*.
//!
//! The resolved view over every producer's and every person's opinion rows
//! in the store, the detector that gives an undescribed table its base
//! layer, and the prompt renderer that puts the resolved model in front of
//! every model call.

pub mod detect;
pub mod handlers;
pub mod prompt;
pub mod types;

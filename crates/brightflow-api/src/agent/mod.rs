//! Agent runs: LLM curation through the SAME action layer humans use.
//!
//! A run builds context for its kind (top insights for triage, ticket
//! summaries for vocabulary induction, column profiles and sample rows for
//! `describe_table`), exposes the relevant slice of the action manifest as
//! tools, and loops the model. Every tool call lands in `dispatch_action`
//! with `Actor::Agent`: a proposal awaiting approval, or applied at once
//! (still undoable) when the run was started in `auto_apply` mode.

pub mod describe;
pub mod handlers;
pub mod runner;
pub mod sampling;
pub mod types;

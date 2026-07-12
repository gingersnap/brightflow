//! Agent runs: LLM curation through the SAME action layer humans use.
//!
//! A run builds context for its kind (cluster terms for auto-label, centroid
//! similarities for merge proposals, top insights for triage), exposes the
//! relevant slice of the action manifest as tools, and loops the model. Every
//! tool call lands in `dispatch_action` with `Actor::Agent` — recorded as a
//! PROPOSAL for human approval, never applied directly.

pub mod handlers;
pub mod runner;
pub mod types;

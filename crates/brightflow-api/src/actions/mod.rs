//! First-class curation actions: one dispatch path for humans and agents.

pub mod handlers;
pub mod types;

pub use handlers::{dispatch_action, execute_action, Actor};
pub use types::{Action, ActionRequest, ActionResponse, ActionStatus};

pub mod backend;
pub mod db;
pub mod error;
pub mod models;
pub mod password;

pub use backend::{AuthBackend, AuthSession, Credentials};
pub use db::AuthDb;
pub use error::{AuthError, AuthResult};
pub use models::{ConnectorConfig, SchedulerJob, SyncRun, SyncState, User, UserSettings};
pub use password::{hash_password, verify_password};

// Re-export session/login crates so API layer doesn't need separate deps
pub use axum_login;
pub use tower_sessions;
pub use tower_sessions_sqlx_store;

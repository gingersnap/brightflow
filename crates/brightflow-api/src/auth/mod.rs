//! Session-cookie authentication: user store, password hashing, the
//! `axum-login` backend, and the login rate limiter.

pub mod backend;
pub mod db;
pub mod error;
pub mod handlers;
pub mod models;
pub mod password;
pub mod rate_limit;
pub mod session_store;

pub use backend::{AuthBackend, AuthSession, Credentials};
pub use db::AuthDb;
pub use error::{AuthError, AuthResult};
pub use models::User;
pub use password::{hash_password, verify_password};
pub use rate_limit::{check_login, client_key, LoginLimiter};
pub use session_store::RusqliteSessionStore;

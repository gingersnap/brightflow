pub mod backend;
pub mod db;
pub mod error;
pub mod handlers;
pub mod models;
pub mod password;

pub use backend::{AuthBackend, AuthSession, Credentials};
pub use db::AuthDb;
pub use error::{AuthError, AuthResult};
pub use models::{User, UserSettings};
pub use password::{hash_password, verify_password};

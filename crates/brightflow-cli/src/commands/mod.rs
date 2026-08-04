//! One module per subcommand family; `main.rs` stays the clap surface and
//! dispatch only.

pub mod admin;
pub mod insights;
pub mod serve;
pub mod store;
pub mod topics;

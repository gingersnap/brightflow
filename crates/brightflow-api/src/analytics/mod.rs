//! Ad-hoc query surface: dataset registry, the Polars query executor, and the
//! REST + WebSocket handlers the query builder drives.

pub mod events_scan;
pub mod executor;
pub mod handlers;
pub mod session;
pub mod types;

//! Server observability: live metrics sampling, process memory, and the log
//! broadcast the system panel streams.

pub mod handlers;
pub mod log_layer;
pub mod proc;
pub mod sampler;

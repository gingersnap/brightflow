//! Finding generators and the tree that ranks them.
//!
//! Each submodule answers one question (is this a trend? an anomaly? a driver?)
//! and emits candidate findings; `scoring`, `select`, and `dedup` decide which
//! survive into a report. The split matters: generators are free to be generous,
//! because selection is what protects the user from a wall of noise.

pub mod anomaly;
pub mod candidates;
pub mod change_point;
pub mod concentration;
pub mod correlation;
pub mod dedup;
pub mod distribution_shift;
pub mod drivers;
pub mod engine;
pub mod forecast;
pub mod history;
pub mod membership;
pub mod null_models;
pub mod outlier_cluster;
pub mod period;
pub mod polarity;
pub mod scoring;
pub mod seasonality;
pub mod segment;
pub mod select;
pub mod tree;
pub mod trend;

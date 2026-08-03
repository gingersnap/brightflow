//! Product-analytics surface (funnels, retention, user timelines) over ingested
//! event data. Split from `web_analytics` because these are per-user questions,
//! not per-pageview ones.

pub mod handlers;
pub mod queries;

//! Which tools each kind of source exposes.
//!
//! A source's kind determines its navigation: event sources get funnels and
//! retention, connector and upload sources get the query and insights tools. This
//! lives server-side so the frontend never has to encode the mapping twice.

use super::types::SourceTool;

/// Tools available for web analytics sources
pub fn web_analytics_tools() -> Vec<SourceTool> {
    vec![
        SourceTool::Dashboard,
        SourceTool::Funnels,
        SourceTool::Retention,
        SourceTool::Users,
        SourceTool::Explore,
        SourceTool::Insights,
    ]
}

/// Tools available for persistent CSV upload sources
pub fn upload_tools() -> Vec<SourceTool> {
    vec![
        SourceTool::Explore,
        SourceTool::Insights,
        SourceTool::Textanalytics,
        SourceTool::Textexplore,
    ]
}

/// Tools available for a connector source, based on connector name
pub fn connector_tools(name: &str) -> Vec<SourceTool> {
    match name {
        "github" => vec![
            SourceTool::Dashboard,
            SourceTool::Explore,
            SourceTool::Insights,
            SourceTool::Textanalytics,
            SourceTool::Textexplore,
        ],
        "bluesky" => vec![
            SourceTool::Explore,
            SourceTool::Insights,
            SourceTool::Textanalytics,
            SourceTool::Textexplore,
        ],
        _ => vec![SourceTool::Explore, SourceTool::Insights],
    }
}

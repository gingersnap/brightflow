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

/// Tools available for a connector source, based on connector name
pub fn connector_tools(name: &str) -> Vec<SourceTool> {
    match name {
        "github" => vec![
            SourceTool::Dashboard,
            SourceTool::Explore,
            SourceTool::Insights,
            SourceTool::Topics,
        ],
        "bluesky" => vec![
            SourceTool::Explore,
            SourceTool::Insights,
            SourceTool::Topics,
        ],
        _ => vec![SourceTool::Explore, SourceTool::Insights],
    }
}

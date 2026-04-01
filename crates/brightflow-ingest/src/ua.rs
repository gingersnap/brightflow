use uaparser::{Parser, UserAgentParser};

/// Parsed User-Agent information.
pub struct UaInfo {
    pub browser: String,
    pub browser_version: String,
    pub os: String,
    pub os_version: String,
    pub device_type: String,
}

/// Bundled UA parser regex definitions (from the ua-parser-project).
const REGEXES_YAML: &[u8] = include_bytes!("../regexes.yaml");

/// Create a UA parser. Call once at startup.
pub fn create_parser() -> UserAgentParser {
    UserAgentParser::from_bytes(REGEXES_YAML).unwrap_or_else(|e| {
        tracing::warn!("Could not load UA parser regexes: {e}. Using empty fallback.");
        UserAgentParser::from_bytes(b"user_agent_parsers: []\nos_parsers: []\ndevice_parsers: []")
            .unwrap_or_else(|_| unreachable!())
    })
}

/// Parse a User-Agent string into browser, OS, and device info.
pub fn parse_ua(parser: &UserAgentParser, ua_string: &str, screen_width: Option<u16>) -> UaInfo {
    let client = parser.parse(ua_string);

    let browser = client.user_agent.family.to_string();
    let browser_version = match (&client.user_agent.major, &client.user_agent.minor) {
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        (Some(major), None) => major.to_string(),
        _ => String::new(),
    };

    let os = client.os.family.to_string();
    let os_version = match (&client.os.major, &client.os.minor) {
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        (Some(major), None) => major.to_string(),
        _ => String::new(),
    };

    let device_type = classify_device(&os, screen_width);

    UaInfo {
        browser,
        browser_version,
        os,
        os_version,
        device_type,
    }
}

/// Classify device type based on OS and screen width.
fn classify_device(os: &str, screen_width: Option<u16>) -> String {
    let is_mobile_os = matches!(os, "Android" | "iOS" | "Windows Phone");

    match (is_mobile_os, screen_width) {
        (true, Some(w)) if w <= 575 => "Mobile".to_string(),
        (true, Some(w)) if w <= 991 => "Tablet".to_string(),
        (true, Some(_)) => "Tablet".to_string(),
        (true, None) => "Mobile".to_string(),
        (false, Some(w)) if w <= 575 => "Mobile".to_string(),
        _ => "Desktop".to_string(),
    }
}

/// Classify screen size into a human-readable category.
#[must_use]
pub fn classify_screen_size(screen_width: Option<u16>) -> String {
    match screen_width {
        Some(w) if w <= 575 => "Small".to_string(),
        Some(w) if w <= 991 => "Medium".to_string(),
        Some(_) => "Large".to_string(),
        None => String::new(),
    }
}

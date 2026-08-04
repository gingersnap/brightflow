//! User-Agent parsing into browser / OS / device.
//!
//! The parser holds a sizable regex set, so it is built once at startup and
//! shared, not constructed per event. Unknown agents yield empty strings rather
//! than an error — an unrecognized browser is normal, not exceptional.

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
const REGEXES_YAML: &[u8] = include_bytes!("../../regexes.yaml");

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
        // Any wider mobile-OS device is a tablet — deliberately including
        // widths above the classic 991 breakpoint, because large tablets in
        // landscape (iPads report >1000px) would otherwise become "Desktop".
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_os_classification_by_width() {
        assert_eq!(classify_device("iOS", Some(375)), "Mobile");
        assert_eq!(classify_device("Android", Some(575)), "Mobile");
        assert_eq!(classify_device("Android", Some(576)), "Tablet");
        assert_eq!(classify_device("iOS", Some(820)), "Tablet");
        // Wide mobile-OS devices stay tablets (iPad landscape), not desktops.
        assert_eq!(classify_device("iOS", Some(1366)), "Tablet");
        // Unknown width on a mobile OS defaults to the common case.
        assert_eq!(classify_device("Android", None), "Mobile");
    }

    #[test]
    fn desktop_os_classification_by_width() {
        assert_eq!(classify_device("Windows", Some(1920)), "Desktop");
        assert_eq!(classify_device("Mac OS X", None), "Desktop");
        // Pins current behavior: a narrow viewport on a desktop OS is
        // classified Mobile.
        assert_eq!(classify_device("Windows", Some(400)), "Mobile");
        assert_eq!(classify_device("Linux", Some(576)), "Desktop");
    }

    #[test]
    fn screen_size_buckets() {
        assert_eq!(classify_screen_size(Some(575)), "Small");
        assert_eq!(classify_screen_size(Some(576)), "Medium");
        assert_eq!(classify_screen_size(Some(991)), "Medium");
        assert_eq!(classify_screen_size(Some(992)), "Large");
        assert_eq!(classify_screen_size(None), "");
    }
}

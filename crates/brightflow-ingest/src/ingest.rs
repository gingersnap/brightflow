use url::Url;

use crate::geo::{self, GeoInfo};
use crate::identity;
use crate::models::{Event, RawEvent};
use crate::ua::{self, UaInfo};

/// Known referrer domain -> source name mappings.
static REFERRER_SOURCES: &[(&str, &str)] = &[
    ("google.", "Google"),
    ("bing.com", "Bing"),
    ("duckduckgo.com", "DuckDuckGo"),
    ("yahoo.", "Yahoo"),
    ("yandex.", "Yandex"),
    ("baidu.com", "Baidu"),
    ("facebook.com", "Facebook"),
    ("fb.com", "Facebook"),
    ("instagram.com", "Instagram"),
    ("twitter.com", "Twitter"),
    ("x.com", "Twitter"),
    ("t.co", "Twitter"),
    ("linkedin.com", "LinkedIn"),
    ("reddit.com", "Reddit"),
    ("youtube.com", "YouTube"),
    ("pinterest.com", "Pinterest"),
    ("tiktok.com", "TikTok"),
    ("github.com", "GitHub"),
    ("news.ycombinator.com", "Hacker News"),
];

/// Parsed UTM parameters.
struct UtmParams {
    source: String,
    medium: String,
    campaign: String,
    content: String,
    term: String,
}

/// Parsed URL components.
struct ParsedUrl {
    hostname: String,
    pathname: String,
    clean_url: String,
    utm: UtmParams,
}

/// Process a raw event into an enriched event.
///
/// This is a pure function (aside from the geo reader). IP and User-Agent
/// are used for hashing/parsing but never stored in the output.
pub fn process_event(
    raw: &RawEvent,
    ip: &str,
    ua_string: &str,
    source_id: &str,
    salt: &str,
    geo_reader: Option<&maxminddb::Reader<Vec<u8>>>,
    ua_parser: &uaparser::UserAgentParser,
) -> Event {
    let id = uuid::Uuid::now_v7().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Visitor ID (hash-based, anonymous)
    let visitor_id = identity::compute_visitor_id(salt, source_id, ip, ua_string);

    // Parse URL
    let parsed = parse_url(&raw.url);

    // Classify referrer
    let referrer_source = classify_referrer(&raw.referrer, &parsed.hostname);

    // Parse User-Agent
    let UaInfo {
        browser,
        browser_version,
        os,
        os_version,
        device_type,
    } = ua::parse_ua(ua_parser, ua_string, raw.screen_width);

    let screen_size = ua::classify_screen_size(raw.screen_width);

    // GeoIP lookup (IP discarded after this)
    let GeoInfo {
        country,
        region,
        city,
    } = geo::lookup_geo(geo_reader, ip);

    // Serialize custom properties
    let properties = raw
        .props
        .as_ref()
        .map_or_else(|| "{}".to_string(), ToString::to_string);

    Event {
        id,
        timestamp,
        source_id: source_id.to_string(),
        event_name: raw.name.clone(),
        visitor_id,
        session_id: String::new(), // Filled in by buffer.insert() after session derivation
        hostname: parsed.hostname,
        pathname: parsed.pathname,
        page_url: parsed.clean_url,
        referrer: raw.referrer.clone(),
        referrer_source,
        utm_source: parsed.utm.source,
        utm_medium: parsed.utm.medium,
        utm_campaign: parsed.utm.campaign,
        utm_content: parsed.utm.content,
        utm_term: parsed.utm.term,
        browser,
        browser_version,
        os,
        os_version,
        device_type,
        screen_size,
        country,
        region,
        city,
        properties,
    }
}

/// Parse URL into hostname, pathname, clean URL (no query params), and UTM parameters.
fn parse_url(raw_url: &str) -> ParsedUrl {
    let Ok(parsed) = Url::parse(raw_url) else {
        return ParsedUrl {
            hostname: String::new(),
            pathname: raw_url.to_string(),
            clean_url: raw_url.to_string(),
            utm: UtmParams {
                source: String::new(),
                medium: String::new(),
                campaign: String::new(),
                content: String::new(),
                term: String::new(),
            },
        };
    };

    let hostname = parsed.host_str().unwrap_or("").to_string();
    let pathname = parsed.path().to_string();

    // Extract UTM params from query string
    let mut utm = UtmParams {
        source: String::new(),
        medium: String::new(),
        campaign: String::new(),
        content: String::new(),
        term: String::new(),
    };

    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "utm_source" => utm.source = value.to_string(),
            "utm_medium" => utm.medium = value.to_string(),
            "utm_campaign" => utm.campaign = value.to_string(),
            "utm_content" => utm.content = value.to_string(),
            "utm_term" => utm.term = value.to_string(),
            _ => {},
        }
    }

    // Build clean URL without query params (privacy)
    let clean_url = format!("{}://{}{}", parsed.scheme(), hostname, pathname);

    ParsedUrl {
        hostname,
        pathname,
        clean_url,
        utm,
    }
}

/// Classify a referrer URL into a human-readable source name.
fn classify_referrer(referrer: &str, current_hostname: &str) -> String {
    if referrer.is_empty() {
        return "Direct".to_string();
    }

    let referrer_host = Url::parse(referrer)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();

    // Same domain = direct (internal navigation)
    if referrer_host == current_hostname {
        return "Direct".to_string();
    }

    // Check known sources
    for (pattern, name) in REFERRER_SOURCES {
        if referrer_host.contains(pattern) {
            return (*name).to_string();
        }
    }

    // Return the hostname itself as the source
    referrer_host
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_extracts_utms() {
        let result =
            parse_url("https://example.com/blog?utm_source=twitter&utm_medium=social&extra=1");
        assert_eq!(result.hostname, "example.com");
        assert_eq!(result.pathname, "/blog");
        assert_eq!(result.clean_url, "https://example.com/blog");
        assert_eq!(result.utm.source, "twitter");
        assert_eq!(result.utm.medium, "social");
    }

    #[test]
    fn classify_referrer_direct() {
        assert_eq!(classify_referrer("", "example.com"), "Direct");
    }

    #[test]
    fn classify_referrer_self() {
        assert_eq!(
            classify_referrer("https://example.com/other", "example.com"),
            "Direct"
        );
    }

    #[test]
    fn classify_referrer_google() {
        assert_eq!(
            classify_referrer("https://www.google.com/search?q=test", "example.com"),
            "Google"
        );
    }

    #[test]
    fn classify_referrer_unknown() {
        assert_eq!(
            classify_referrer("https://myblog.dev/post/1", "example.com"),
            "myblog.dev"
        );
    }
}

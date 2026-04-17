use std::net::IpAddr;
use std::path::Path;

/// GeoIP lookup result.
#[derive(Default)]
pub struct GeoInfo {
    pub country: String,
    pub region: String,
    pub city: String,
}

/// Load the MaxMind GeoLite2-City database. Returns None if not found.
pub fn load_geoip_reader(geoip_path: &Path) -> Option<maxminddb::Reader<Vec<u8>>> {
    let path = std::env::var("BRIGHTFLOW_GEOIP_DB").map_or_else(
        |_| geoip_path.join("geolite2-city.mmdb"),
        std::path::PathBuf::from,
    );

    if let Ok(reader) = maxminddb::Reader::open_readfile(&path) {
        tracing::info!("Loaded GeoIP database from {}", path.display());
        Some(reader)
    } else {
        tracing::info!(
            "GeoIP database not found at {} \u{2014} geo enrichment disabled",
            path.display()
        );
        None
    }
}

/// Look up geographic information for an IP address.
pub fn lookup_geo(reader: Option<&maxminddb::Reader<Vec<u8>>>, ip: &str) -> GeoInfo {
    let Some(reader) = reader else {
        return GeoInfo::default();
    };

    let Ok(ip_addr) = ip.parse::<IpAddr>() else {
        return GeoInfo::default();
    };

    let Ok(city) = reader.lookup::<maxminddb::geoip2::City<'_>>(ip_addr) else {
        return GeoInfo::default();
    };

    let country = city
        .country
        .as_ref()
        .and_then(|c| c.iso_code)
        .unwrap_or("")
        .to_string();

    let region = city
        .subdivisions
        .as_ref()
        .and_then(|s| s.first())
        .and_then(|s| s.names.as_ref())
        .and_then(|n| n.get("en"))
        .copied()
        .unwrap_or("")
        .to_string();

    let city_name = city
        .city
        .as_ref()
        .and_then(|c| c.names.as_ref())
        .and_then(|n| n.get("en"))
        .copied()
        .unwrap_or("")
        .to_string();

    GeoInfo {
        country,
        region,
        city: city_name,
    }
}

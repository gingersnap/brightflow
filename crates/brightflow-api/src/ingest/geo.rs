//! Optional GeoIP enrichment via a local MaxMind database.
//!
//! Entirely optional and fails soft: a missing database, an unparseable address,
//! or an IP the database doesn't cover all yield empty geo rather than dropping
//! the event. Location is nice to have; the pageview is the thing that matters.
//!
//! Uses `open_readfile` rather than `open_mmap` deliberately — the mmap path is
//! the one carrying RUSTSEC-2025-0132's unsoundness.

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

    // maxminddb 0.27 defers decoding: `lookup` returns a handle, `decode` yields
    // `Ok(None)` for an IP the database simply doesn't cover. Both the lookup error and
    // the miss degrade to empty geo rather than dropping the event.
    let Ok(lookup) = reader.lookup(ip_addr) else {
        return GeoInfo::default();
    };
    let Ok(Some(city)) = lookup.decode::<maxminddb::geoip2::City<'_>>() else {
        return GeoInfo::default();
    };

    let country = city.country.iso_code.unwrap_or("").to_string();

    // Subdivisions run largest-to-smallest, so the first entry is the state/province.
    let region = city
        .subdivisions
        .first()
        .and_then(|s| s.names.english)
        .unwrap_or("")
        .to_string();

    let city_name = city.city.names.english.unwrap_or("").to_string();

    GeoInfo {
        country,
        region,
        city: city_name,
    }
}

#![allow(clippy::cognitive_complexity, clippy::shadow_reuse)]

pub mod buffer;
pub mod db;
pub mod error;
pub mod flush;
pub mod geo;
pub mod identity;
pub mod ingest;
pub mod models;
pub mod script;
pub mod ua;

use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::RwLock;

use crate::buffer::EventBuffer;
use crate::db::IngestDb;
use crate::error::IngestResult;
use crate::models::Source;

/// Shared ingest state held in the API server's `AppState`.
pub struct IngestState {
    /// Central metadata database (sources, salts).
    pub db: IngestDb,
    /// Per-source SQLite buffer pools.
    pub buffer: Arc<EventBuffer>,
    /// In-memory cache: domain → Source (avoids DB lookup per event).
    pub source_cache: DashMap<String, Source>,
    /// Cached daily salt: (date, salt). Refreshed once per day.
    pub salt_cache: RwLock<(String, String)>,
    /// GeoIP reader (None if MaxMind DB not found).
    pub geo_reader: Option<maxminddb::Reader<Vec<u8>>>,
    /// User-Agent parser (initialized once at startup).
    pub ua_parser: uaparser::UserAgentParser,
}

impl IngestState {
    /// Create new ingest state.
    pub async fn new(
        db: IngestDb,
        buffer: EventBuffer,
        geo_reader: Option<maxminddb::Reader<Vec<u8>>>,
        ua_parser: uaparser::UserAgentParser,
    ) -> IngestResult<Self> {
        let state = Self {
            db,
            buffer: Arc::new(buffer),
            source_cache: DashMap::new(),
            salt_cache: RwLock::new((String::new(), String::new())),
            geo_reader,
            ua_parser,
        };
        state.refresh_source_cache().await?;
        Ok(state)
    }

    /// Reload all sources from the database into the in-memory cache.
    pub async fn refresh_source_cache(&self) -> IngestResult<()> {
        let sources = self.db.list_sources().await?;
        self.source_cache.clear();
        for source in sources {
            self.source_cache.insert(source.domain.clone(), source);
        }
        Ok(())
    }

    /// Look up a source by domain from the in-memory cache.
    pub fn get_source_by_domain(&self, domain: &str) -> Option<Source> {
        self.source_cache.get(domain).map(|s| s.clone())
    }

    /// Get today's salt, using the cache when possible.
    pub async fn get_today_salt(&self) -> IngestResult<String> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // Check cache
        {
            let cache = self.salt_cache.read().await;
            if cache.0 == today {
                return Ok(cache.1.clone());
            }
        }

        // Cache miss — fetch/create from DB
        let salt = self.db.get_or_create_salt(&today).await?;
        {
            let mut cache = self.salt_cache.write().await;
            *cache = (today, salt.clone());
        }

        Ok(salt)
    }
}

/// Initialize the ingest system. Call during server startup.
pub async fn init(
    ingest_url: &str,
    buffer_dir: &Path,
    geoip_base_path: &Path,
) -> IngestResult<IngestState> {
    let db = IngestDb::new(ingest_url).await?;
    let buffer = EventBuffer::new(buffer_dir.to_path_buf());
    let geo_reader = geo::load_geoip_reader(geoip_base_path);
    let ua_parser = ua::create_parser();

    IngestState::new(db, buffer, geo_reader, ua_parser).await
}

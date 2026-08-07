//! rusqlite-backed tower-sessions store over the auth pool, replacing
//! `tower-sessions-sqlx-store`.
//!
//! Records are stored as rmp-serde blobs of the whole `Record` — session ids
//! are full-range `i128`, which serde_json cannot represent, so MessagePack
//! is load-bearing here, not a taste choice (it also matches what the
//! replaced sqlx store wrote). Expiry lives in its own unix-seconds column so
//! lookups and the sweeper compare integers instead of parsing datetimes.
//! The `tower_sessions` schema is auth migration 006; this module only reads
//! and writes it.

use async_trait::async_trait;
use brightflow_store::rusqlite::params;
use brightflow_store::SqlitePool;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::{self, ExpiredDeletion};
use tower_sessions::SessionStore;

#[derive(Clone, Debug)]
pub struct RusqliteSessionStore {
    pool: SqlitePool,
}

impl RusqliteSessionStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn encode(record: &Record) -> session_store::Result<Vec<u8>> {
    rmp_serde::to_vec(record).map_err(|e| session_store::Error::Encode(e.to_string()))
}

fn backend(e: brightflow_store::SqliteError) -> session_store::Error {
    session_store::Error::Backend(e.to_string())
}

#[async_trait]
impl SessionStore for RusqliteSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        // Insert with OR ABORT and regenerate the id on a PK collision —
        // the same collision-mitigation loop the upstream stores implement.
        loop {
            let id = record.id.to_string();
            let data = encode(record)?;
            let expiry = record.expiry_date.unix_timestamp();
            let inserted = self
                .pool
                .call(move |conn| {
                    match conn.execute(
                        "INSERT OR ABORT INTO tower_sessions (id, data, expiry_date)
                         VALUES (?, ?, ?)",
                        params![id, data, expiry],
                    ) {
                        Ok(_) => Ok(true),
                        Err(brightflow_store::rusqlite::Error::SqliteFailure(e, _))
                            if e.extended_code
                                == brightflow_store::rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
                        {
                            Ok(false)
                        }
                        Err(e) => Err(e),
                    }
                })
                .await
                .map_err(backend)?;
            if inserted {
                return Ok(());
            }
            record.id = Id::default();
        }
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let id = record.id.to_string();
        let data = encode(record)?;
        let expiry = record.expiry_date.unix_timestamp();
        self.pool
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO tower_sessions (id, data, expiry_date)
                     VALUES (?, ?, ?)
                     ON CONFLICT(id) DO UPDATE SET
                        data = excluded.data,
                        expiry_date = excluded.expiry_date",
                    params![id, data, expiry],
                )
                .map(|_| ())
            })
            .await
            .map_err(backend)
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let id = session_id.to_string();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let blob: Option<Vec<u8>> = self
            .pool
            .call(move |conn| {
                brightflow_store::fetch_optional::<(Vec<u8>,), _>(
                    conn,
                    "SELECT data FROM tower_sessions WHERE id = ? AND expiry_date > ?",
                    params![id, now],
                )
                .map(|row| row.map(|(data,)| data))
            })
            .await
            .map_err(backend)?;
        blob.map(|data| {
            rmp_serde::from_slice(&data).map_err(|e| session_store::Error::Decode(e.to_string()))
        })
        .transpose()
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let id = session_id.to_string();
        self.pool
            .call(move |conn| {
                conn.execute("DELETE FROM tower_sessions WHERE id = ?", params![id])
                    .map(|_| ())
            })
            .await
            .map_err(backend)
    }
}

#[async_trait]
impl ExpiredDeletion for RusqliteSessionStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        self.pool
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM tower_sessions WHERE expiry_date < ?",
                    params![now],
                )
                .map(|_| ())
            })
            .await
            .map_err(backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthDb;
    use std::collections::HashMap;

    async fn temp_store(dir: &tempfile::TempDir) -> RusqliteSessionStore {
        let url = format!("sqlite:{}?mode=rwc", dir.path().join("auth.db").display());
        let db = AuthDb::new(&url).await.expect("auth db");
        RusqliteSessionStore::new(db.pool().clone())
    }

    fn record(expiry_offset_secs: i64) -> Record {
        let mut data = HashMap::new();
        data.insert("k".to_owned(), serde_json::json!({"v": 1}));
        Record {
            id: Id::default(),
            data,
            expiry_date: time::OffsetDateTime::now_utc()
                + time::Duration::seconds(expiry_offset_secs),
        }
    }

    #[tokio::test]
    async fn create_load_round_trip_preserves_record() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = temp_store(&dir).await;
        let mut rec = record(3600);
        store.create(&mut rec).await.expect("create");

        let loaded = store
            .load(&rec.id)
            .await
            .expect("load")
            .expect("session exists");
        assert_eq!(loaded.id, rec.id);
        assert_eq!(loaded.data, rec.data);
        assert_eq!(
            loaded.expiry_date.unix_timestamp(),
            rec.expiry_date.unix_timestamp()
        );
    }

    #[tokio::test]
    async fn expired_sessions_do_not_load_and_get_swept() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = temp_store(&dir).await;
        let mut rec = record(-60);
        store.create(&mut rec).await.expect("create");

        assert!(
            store.load(&rec.id).await.expect("load").is_none(),
            "expired session must not load"
        );

        store.delete_expired().await.expect("sweep");
        let remaining: i64 = store
            .pool
            .call(|conn| conn.query_row("SELECT count(*) FROM tower_sessions", [], |r| r.get(0)))
            .await
            .expect("count");
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn create_regenerates_id_on_collision() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = temp_store(&dir).await;
        let mut first = record(3600);
        store.create(&mut first).await.expect("create first");

        let mut second = record(3600);
        second.id = first.id;
        store.create(&mut second).await.expect("create second");
        assert_ne!(second.id, first.id, "collision must mint a fresh id");
        assert!(store.load(&second.id).await.expect("load").is_some());
    }

    #[tokio::test]
    async fn save_upserts_and_delete_removes() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = temp_store(&dir).await;
        let mut rec = record(3600);
        store.create(&mut rec).await.expect("create");

        rec.data
            .insert("extra".to_owned(), serde_json::json!("updated"));
        store.save(&rec).await.expect("save");
        let loaded = store.load(&rec.id).await.expect("load").expect("exists");
        assert_eq!(
            loaded.data.get("extra"),
            Some(&serde_json::json!("updated"))
        );

        store.delete(&rec.id).await.expect("delete");
        assert!(store.load(&rec.id).await.expect("load").is_none());
    }
}

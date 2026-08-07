-- Session persistence for tower-sessions, owned in-tree since the move off
-- tower-sessions-sqlx-store (which created this table out-of-band).
-- data is the rmp-serde encoding of the whole session record; expiry_date is
-- unix seconds so expiry comparisons are integer, never datetime() parsing.
CREATE TABLE IF NOT EXISTS tower_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    data BLOB NOT NULL,
    expiry_date INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tower_sessions_expiry ON tower_sessions(expiry_date);

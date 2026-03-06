CREATE TABLE IF NOT EXISTS sync_state (
    connector_id TEXT NOT NULL REFERENCES connector_configs(id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL,
    cursor_field TEXT,
    cursor_value TEXT,
    last_sync_at TEXT,
    last_sync_status TEXT,
    rows_synced INTEGER DEFAULT 0,
    PRIMARY KEY (connector_id, endpoint)
);

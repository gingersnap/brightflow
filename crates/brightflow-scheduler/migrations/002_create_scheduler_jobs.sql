CREATE TABLE IF NOT EXISTS scheduler_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    connector_id TEXT NOT NULL REFERENCES connector_configs(id) ON DELETE CASCADE,
    interval_secs INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

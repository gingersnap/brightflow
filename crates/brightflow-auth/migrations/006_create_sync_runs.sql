CREATE TABLE IF NOT EXISTS sync_runs (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT REFERENCES scheduler_jobs(id) ON DELETE SET NULL,
    connector_id TEXT NOT NULL REFERENCES connector_configs(id),
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    endpoints_synced TEXT,
    rows_synced INTEGER DEFAULT 0,
    error TEXT
);

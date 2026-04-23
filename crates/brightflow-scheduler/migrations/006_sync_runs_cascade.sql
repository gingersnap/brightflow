-- Cascade sync_runs when a connector_config is deleted.
-- Previously sync_runs.connector_id had no ON DELETE action, so deleting a
-- preset with any historical runs triggered a FOREIGN KEY constraint failure.

CREATE TABLE sync_runs_new (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT REFERENCES scheduler_jobs(id) ON DELETE SET NULL,
    connector_id TEXT NOT NULL REFERENCES connector_configs(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    endpoints_synced TEXT,
    rows_synced INTEGER DEFAULT 0,
    error TEXT
);

INSERT INTO sync_runs_new
    (id, job_id, connector_id, started_at, finished_at, status, endpoints_synced, rows_synced, error)
SELECT
    id, job_id, connector_id, started_at, finished_at, status, endpoints_synced, rows_synced, error
FROM sync_runs;

DROP TABLE sync_runs;
ALTER TABLE sync_runs_new RENAME TO sync_runs;

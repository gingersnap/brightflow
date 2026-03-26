CREATE TABLE IF NOT EXISTS table_files (
    id TEXT PRIMARY KEY NOT NULL,
    table_id TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    num_rows INTEGER NOT NULL DEFAULT 0,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_table_files_table_id ON table_files(table_id);

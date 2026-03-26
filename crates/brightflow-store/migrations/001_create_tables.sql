CREATE TABLE IF NOT EXISTS tables (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT UNIQUE NOT NULL,
    version INTEGER NOT NULL DEFAULT 0,
    schema_json TEXT,
    primary_keys TEXT,
    total_rows INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

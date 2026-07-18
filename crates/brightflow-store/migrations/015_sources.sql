-- Store-level source registry for sources that have no connector row,
-- starting with persistent CSV uploads. Connector/web sources keep living
-- in their own registries; source_id stays an opaque string everywhere else.
CREATE TABLE IF NOT EXISTS sources (
    source_id  TEXT PRIMARY KEY NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('upload')),
    name       TEXT NOT NULL,
    meta_json  TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Table-level text-enrichment settings (overrides the builtin per-table defaults)
CREATE TABLE IF NOT EXISTS table_enrichment_settings (
    table_id         TEXT PRIMARY KEY NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    -- JSON array of text column names to embed, e.g. ["title","body"]
    text_columns     TEXT,
    cleaning_profile TEXT CHECK (cleaning_profile IN ('social','markdown_issue','plain')),
    language_column  TEXT,
    embedder         TEXT,
    min_cluster_size INTEGER,
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

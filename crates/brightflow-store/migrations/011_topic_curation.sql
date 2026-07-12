-- Topic curation overlay: durable edits applied at read time over fitted
-- clusters. `fit_topics` regenerates clusters.bin wholesale, so edits attach
-- to a centroid snapshot and are reconciled onto new clusters after each
-- re-fit via cosine similarity (>= 0.80 reattach, else orphaned).
CREATE TABLE IF NOT EXISTS cluster_edits (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id             TEXT NOT NULL,
    -- Stable hash of the centroid this edit attaches to
    centroid_fingerprint TEXT NOT NULL,
    -- Snapshot of the centroid vector (JSON array) for reconciliation
    centroid_json        TEXT NOT NULL,
    -- Raw cluster id in the CURRENT fit (updated on reconcile); NULL = orphaned
    cluster_id           INTEGER,
    custom_name          TEXT,
    label                TEXT,
    is_noise             INTEGER NOT NULL DEFAULT 0,
    -- Raw cluster id this cluster is merged into (current fit)
    merged_into          INTEGER,
    orphaned             INTEGER NOT NULL DEFAULT 0,
    updated_at           INTEGER NOT NULL,
    UNIQUE (table_id, centroid_fingerprint)
);

-- Terms excluded from cluster naming / top-term lists for a table.
CREATE TABLE IF NOT EXISTS excluded_terms (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id   TEXT NOT NULL,
    term       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (table_id, term)
);

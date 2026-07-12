-- Insight history: which stories have been shown, for novelty decay.
-- Fingerprints are stable FNV-1a hashes of the insight recipe
-- (type|measure|agg|derivations|filters|granularity) — values/periods excluded
-- so "the same story next week" matches.
CREATE TABLE IF NOT EXISTS insight_history (
    table_id       TEXT NOT NULL,
    fingerprint    TEXT NOT NULL,
    -- Human-readable identity (provenance label) for inspection/debugging
    identity       TEXT NOT NULL DEFAULT '',
    insight_type   TEXT NOT NULL DEFAULT '',
    -- Direction + magnitude decile of the last shown value; a change resets novelty
    last_value_sig TEXT NOT NULL DEFAULT '',
    shown_count    INTEGER NOT NULL DEFAULT 0,
    first_shown_at INTEGER NOT NULL,
    last_shown_at  INTEGER NOT NULL,
    PRIMARY KEY (table_id, fingerprint)
);

-- User curation of individual insights (dismiss / pin), keyed by fingerprint.
CREATE TABLE IF NOT EXISTS insight_state (
    table_id    TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    state       TEXT NOT NULL CHECK (state IN ('dismissed','pinned')),
    -- For dismissals: 'boring' | 'known' | 'wrong'
    reason      TEXT,
    annotation  TEXT,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (table_id, fingerprint)
);

-- Broad suppressions: never surface insights about this segment or column.
CREATE TABLE IF NOT EXISTS insight_suppressions (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id   TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('segment','column')),
    target     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (table_id, kind, target)
);

-- Column-level semantic overrides (user-defined roles for insights analysis)
CREATE TABLE IF NOT EXISTS column_semantics (
    table_id    TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    column_name TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('measure','dimension','time','entity','ignored')),
    is_kpi      INTEGER NOT NULL DEFAULT 0,
    label       TEXT,
    description TEXT,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (table_id, column_name)
);

-- Table-level analysis settings overrides
CREATE TABLE IF NOT EXISTS table_analysis_settings (
    table_id            TEXT PRIMARY KEY NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    display_name        TEXT,
    description         TEXT,
    time_granularity    TEXT CHECK (time_granularity IN ('day','week','month','quarter','year')),
    comparison_periods  INTEGER,
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

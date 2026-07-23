-- One row per insights computation (manual or post-sync) — powers the
-- new-findings badge and the run history panel. `triggered_by`, not
-- `trigger`: TRIGGER is an SQLite keyword.
CREATE TABLE insight_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  table_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  table_name TEXT NOT NULL,
  report_type TEXT NOT NULL,
  triggered_by TEXT NOT NULL CHECK (triggered_by IN ('manual', 'post_sync')),
  finding_count INTEGER NOT NULL,
  new_finding_count INTEGER NOT NULL,
  top_summary TEXT,
  execution_time_ms REAL NOT NULL,
  computed_at INTEGER NOT NULL
);

CREATE INDEX idx_insight_runs_table ON insight_runs (table_id, computed_at DESC);

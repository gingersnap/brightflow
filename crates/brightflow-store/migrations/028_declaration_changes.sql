-- What a producer's re-declaration changed: one row per apply that differed
-- from the same producer's previous rows, with the field-level diff as JSON
-- (brightflow-types DeclarationChange[]). Read by the UI to show what a
-- connector upgrade changed; never consulted for resolution.
CREATE TABLE declaration_changes (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id     TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    producer     TEXT NOT NULL,
    from_version TEXT,
    to_version   TEXT,
    changes_json TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX declaration_changes_table ON declaration_changes(table_id, id);

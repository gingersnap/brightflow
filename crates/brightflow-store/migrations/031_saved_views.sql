-- Saved views: a named Explore configuration over one table, stored as the
-- client's own JSON so the builder can grow without a migration per field.
-- Created, renamed and deleted through the action bus, so every change is
-- logged and undoable. A table's views go with it.

CREATE TABLE saved_views (
    id          TEXT PRIMARY KEY NOT NULL,
    table_id    TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('explore')),
    spec_json   TEXT NOT NULL,
    created_by  TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (table_id, name)
);

CREATE INDEX saved_views_table_idx ON saved_views(table_id, name);

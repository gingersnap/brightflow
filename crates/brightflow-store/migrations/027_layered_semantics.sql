-- Layered semantics: one row per (object, layer, producer), resolved per
-- field by the reader in layer precedence (user > agent > declared >
-- detected). A producer's re-declaration rewrites only its own rows; a
-- person's edit stays on top. Vocabulary literals in the CHECKs mirror the
-- enums in brightflow-types; a store test asserts the two agree.

-- Column semantics -----------------------------------------------------------
CREATE TABLE column_semantics_new (
    table_id         TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    column_name      TEXT NOT NULL,
    layer            TEXT NOT NULL CHECK (layer IN ('detected','declared','agent','user')),
    producer         TEXT NOT NULL,
    producer_version TEXT,
    producer_hash    TEXT,
    datatype         TEXT CHECK (datatype IS NULL OR datatype IN
                       ('String','Integer','Decimal','Float','Boolean','Date','Time','DateTime','DateTimeTz','Opaque')),
    is_time          INTEGER,
    role             TEXT CHECK (role IS NULL OR role IN ('measure','dimension','time','entity','ignored')),
    is_kpi           INTEGER,
    polarity         TEXT CHECK (polarity IS NULL OR polarity IN ('higher_is_better','lower_is_better','neutral')),
    label            TEXT,
    description      TEXT,
    ai_context_json  TEXT,
    extensions_json  TEXT,
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (table_id, column_name, layer, producer)
);

-- Existing rows: a row a person (or an agent) touched through the action bus
-- becomes that layer's row; everything else — the GitHub seed, enrichment's
-- declarations — becomes a legacy declaration that the next sync or run
-- supersedes.
INSERT INTO column_semantics_new
    (table_id, column_name, layer, producer, role, is_kpi, polarity, label, description, updated_at)
SELECT
    cs.table_id,
    cs.column_name,
    CASE
        WHEN EXISTS (
            SELECT 1 FROM action_log al JOIN tables t ON t.id = cs.table_id
            WHERE al.status IN ('applied','undone')
              AND al.action_kind IN ('set_kpi','set_column_polarity','set_column_role','set_column_label','set_column_description')
              AND json_extract(al.params_json, '$.column') = cs.column_name
              AND json_extract(al.params_json, '$.scope.source_id') = t.source_id
              AND json_extract(al.params_json, '$.scope.table') = t.name
              AND al.actor_type = 'human'
        ) THEN 'user'
        WHEN EXISTS (
            SELECT 1 FROM action_log al JOIN tables t ON t.id = cs.table_id
            WHERE al.status IN ('applied','undone')
              AND al.action_kind IN ('set_kpi','set_column_polarity','set_column_role','set_column_label','set_column_description')
              AND json_extract(al.params_json, '$.column') = cs.column_name
              AND json_extract(al.params_json, '$.scope.source_id') = t.source_id
              AND json_extract(al.params_json, '$.scope.table') = t.name
              AND al.actor_type = 'agent'
        ) THEN 'agent'
        ELSE 'declared'
    END,
    CASE
        WHEN EXISTS (
            SELECT 1 FROM action_log al JOIN tables t ON t.id = cs.table_id
            WHERE al.status IN ('applied','undone')
              AND al.action_kind IN ('set_kpi','set_column_polarity','set_column_role','set_column_label','set_column_description')
              AND json_extract(al.params_json, '$.column') = cs.column_name
              AND json_extract(al.params_json, '$.scope.source_id') = t.source_id
              AND json_extract(al.params_json, '$.scope.table') = t.name
        ) THEN 'legacy:action_log'
        ELSE 'legacy'
    END,
    cs.role,
    cs.is_kpi,
    cs.polarity,
    cs.label,
    cs.description,
    cs.updated_at
FROM column_semantics cs;

DROP TABLE column_semantics;
ALTER TABLE column_semantics_new RENAME TO column_semantics;

-- Table semantics ------------------------------------------------------------
-- Replaces table_analysis_settings. Its rows had no writer but the seed and a
-- PUT route with no caller, so they migrate as legacy declarations.
CREATE TABLE table_semantics (
    table_id           TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    layer              TEXT NOT NULL CHECK (layer IN ('detected','declared','agent','user')),
    producer           TEXT NOT NULL,
    producer_version   TEXT,
    producer_hash      TEXT,
    display_name       TEXT,
    description        TEXT,
    time_granularity   TEXT CHECK (time_granularity IS NULL OR time_granularity IN ('day','week','month','quarter','year')),
    comparison_periods INTEGER,
    doc_json           TEXT,
    ai_context_json    TEXT,
    extensions_json    TEXT,
    updated_at         TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (table_id, layer, producer)
);

INSERT INTO table_semantics
    (table_id, layer, producer, display_name, description, time_granularity, comparison_periods, updated_at)
SELECT table_id, 'declared', 'legacy', display_name, description, time_granularity, comparison_periods, updated_at
FROM table_analysis_settings;

DROP TABLE table_analysis_settings;

-- Relationships --------------------------------------------------------------
-- Many-to-one, from the many side to the one side, within one source.
CREATE TABLE relationships (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id        TEXT NOT NULL,
    name             TEXT NOT NULL,
    from_table_id    TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    to_table_id      TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    from_columns_json TEXT NOT NULL,
    to_columns_json  TEXT NOT NULL,
    layer            TEXT NOT NULL CHECK (layer IN ('detected','declared','agent','user')),
    producer         TEXT NOT NULL,
    producer_version TEXT,
    ai_context_json  TEXT,
    extensions_json  TEXT,
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (source_id, name, layer, producer)
);

-- Metrics --------------------------------------------------------------------
-- One aggregation over one column of one table, kept structured; `sql` is
-- the rendered ANSI form for export only.
CREATE TABLE metrics (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id         TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    expr_json        TEXT NOT NULL,
    sql              TEXT NOT NULL,
    datatype         TEXT CHECK (datatype IS NULL OR datatype IN
                       ('String','Integer','Decimal','Float','Boolean','Date','Time','DateTime','DateTimeTz','Opaque')),
    description      TEXT,
    is_kpi           INTEGER,
    polarity         TEXT CHECK (polarity IS NULL OR polarity IN ('higher_is_better','lower_is_better','neutral')),
    format           TEXT,
    layer            TEXT NOT NULL CHECK (layer IN ('detected','declared','agent','user')),
    producer         TEXT NOT NULL,
    producer_version TEXT,
    ai_context_json  TEXT,
    extensions_json  TEXT,
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (table_id, name, layer, producer)
);

-- Sources ---------------------------------------------------------------------
-- Connector and web sources join uploads here, so a table's producer and its
-- version are known to the store without consulting the scheduler database.
CREATE TABLE sources_new (
    source_id        TEXT PRIMARY KEY NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('upload','connector','web')),
    name             TEXT NOT NULL,
    meta_json        TEXT,
    producer         TEXT,
    producer_version TEXT,
    producer_hash    TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO sources_new (source_id, kind, name, meta_json, created_at)
SELECT source_id, kind, name, meta_json, created_at FROM sources;
DROP TABLE sources;
ALTER TABLE sources_new RENAME TO sources;

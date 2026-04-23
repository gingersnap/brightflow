-- Source-scoped table layout: tables are uniquely identified by (source_id, name).
-- Wipes all catalog metadata so on-disk parquet can be resynced under the new layout.
DELETE FROM column_semantics;
DELETE FROM table_analysis_settings;
DELETE FROM file_column_stats;
DELETE FROM file_partitions;
DELETE FROM table_column_stats;
DELETE FROM table_files;
DELETE FROM tables;

-- SQLite can't drop constraints in place; rebuild `tables` with composite UNIQUE
-- and NOT NULL source_id.
CREATE TABLE tables_new (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    source_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 0,
    schema_json TEXT,
    primary_keys TEXT,
    partition_columns TEXT,
    total_rows INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (source_id, name)
);
DROP TABLE tables;
ALTER TABLE tables_new RENAME TO tables;
CREATE INDEX tables_source_id_idx ON tables(source_id);

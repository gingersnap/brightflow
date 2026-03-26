CREATE TABLE IF NOT EXISTS table_column_stats (
    table_id TEXT NOT NULL REFERENCES tables(id) ON DELETE CASCADE,
    column_name TEXT NOT NULL,
    min_value TEXT,
    max_value TEXT,
    null_count INTEGER,
    PRIMARY KEY (table_id, column_name)
);

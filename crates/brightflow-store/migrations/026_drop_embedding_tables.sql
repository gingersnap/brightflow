-- The embedding / clustering / classifier-head path is gone: nothing reads
-- cluster edits, excluded naming terms, row-level training labels, or the
-- pre-function per-table enrichment settings any more. Dropped rather than
-- left as dead tables so a future reader cannot mistake them for live state.

DROP TABLE IF EXISTS document_labels;
DROP TABLE IF EXISTS cluster_edits;
DROP TABLE IF EXISTS excluded_terms;
DROP TABLE IF EXISTS table_enrichment_settings;

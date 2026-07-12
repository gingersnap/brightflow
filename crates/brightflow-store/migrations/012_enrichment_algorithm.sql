-- Clustering algorithm as a persistent per-table enrichment setting
-- (was per-recluster-request only; settings must be GUI-configurable).
ALTER TABLE table_enrichment_settings
    ADD COLUMN algorithm TEXT CHECK (algorithm IN ('kmeans', 'hdbscan'));

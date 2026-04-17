# Litehouse Research

Deep research on the Litehouse lakehouse architecture, the broader lakehouse ecosystem, and adjacent technologies that inform its design.

**Date**: 2026-04-15

---

## Table of Contents

1. [Litehouse Current State](#1-litehouse-current-state)
2. [Apache Iceberg](#2-apache-iceberg)
3. [Delta Lake](#3-delta-lake)
4. [Apache Hudi](#4-apache-hudi)
5. [DuckLake](#5-ducklake)
6. [The Post-Modern Data Stack](#6-the-post-modern-data-stack)
7. [SQLite as a Metadata Catalog](#7-sqlite-as-a-metadata-catalog)
8. [Polars as a Query Engine](#8-polars-as-a-query-engine)
9. [Adjacent Projects](#9-adjacent-projects)
10. [Key Lakehouse Design Patterns](#10-key-lakehouse-design-patterns)
11. [Recommendations for Litehouse](#11-recommendations-for-litehouse)

---

## 1. Litehouse Current State

Litehouse lives in the `brightflow-store` crate. It is a SQLite-backed lakehouse combining:

- **Metadata Catalog**: SQLite database tracking tables, files, partitions, and column statistics
- **Data Storage**: Parquet files organized in a filesystem hierarchy
- **Query Engine**: Polars for lazy evaluation and batch processing

### Core Structure

The main struct is `ParquetStore`, wrapping:
- `root_path` -- filesystem root for all Parquet files
- `db` -- SQLite connection pool (`StoreDb`) for metadata

### SQLite Schema (6 migrations)

**`tables`** -- Core table registry
- `id TEXT PRIMARY KEY` (UUIDv7)
- `name TEXT UNIQUE`
- `version INTEGER` (incremented on mutations)
- `schema_json TEXT` (Polars schema as JSON)
- `primary_keys TEXT` (JSON array)
- `total_rows INTEGER`
- `partition_columns TEXT` (JSON array)
- `source_id TEXT` (foreign key: `web:{id}`, `connector:{id}`)
- `created_at`, `updated_at`

**`table_files`** -- Parquet file registry
- `id TEXT PRIMARY KEY` (UUIDv7)
- `table_id TEXT` (FK with CASCADE)
- `path TEXT` (relative or absolute)
- `num_rows INTEGER`, `size_bytes INTEGER`
- `added_at TEXT`
- Index on `table_id`

**`table_column_stats`** -- Table-level aggregated statistics
- Composite PK: `(table_id, column_name)`
- `min_value TEXT`, `max_value TEXT`, `null_count INTEGER`

**`file_partitions`** -- Per-file partition key/value pairs
- Composite PK: `(file_id, partition_key)`
- `partition_value TEXT`
- Index on `(partition_key, partition_value)` for pruning

**`file_column_stats`** -- Per-file column statistics
- Composite PK: `(file_id, column_name)`
- `min_value TEXT`, `max_value TEXT`, `null_count INTEGER`
- Enables file-level range pruning

**`column_semantics`** -- Semantic layer overrides
- Composite PK: `(table_id, column_name)`
- `role TEXT` (measure, dimension, time, entity, ignored)
- `is_kpi BOOLEAN`, `label TEXT`, `description TEXT`

**`table_analysis_settings`** -- Table-level analysis config
- `table_id TEXT PRIMARY KEY`
- `display_name TEXT`, `description TEXT`
- `time_granularity TEXT` (day/week/month/quarter/year)
- `comparison_periods INTEGER`

### SQLite Configuration

- Journal mode: WAL (concurrent readers)
- Foreign keys: enabled with CASCADE
- `synchronous=NORMAL`, `cache_size=-64000` (64 MB)
- `mmap_size=268435456` (256 MB memory-mapped I/O)
- `temp_store=MEMORY`, `busy_timeout=5s`
- Connection pool: max 5 (sqlx)
- Migrations auto-run on startup

### Filesystem Layout

```
root_path/
  {table_name}/
    {uuid-v7}.parquet
  events_{source_id}/
    {date}/
      {uuid-v7}.parquet
  litehouse.db
```

File naming uses UUIDv7 (includes timestamp). Compression: Zstd.

### Core Operations

**Ingest** (`ingest_parquet`): Append, overwrite, error-if-exists, or ignore. Copies Parquet to `root_path/{table}/{uuid}.parquet`, extracts metadata (row count, schema, file size, column stats), and registers in SQLite.

**Merge** (`merge_parquet`): Upsert by primary key. Reads all existing data + new data via Polars, performs `semi_join` (find updates) and `anti_join` (unchanged rows), concatenates, writes new file, atomically replaces old file records. Returns `{rows_updated, rows_inserted}`.

**Scan** (`scan_table`): Returns `LazyFrame`. Queries SQLite for matching files via partition and column-stat pruning, then calls `LazyFrame::scan_parquet_files()` on the matching subset. Supports `PartitionEq`, `PartitionRange`, and `ColumnRange` filters.

**Compact** (`compact_partition`): Merges all files in a partition into one. Reads via Polars concat, writes new file, replaces metadata in transaction, deletes old physical files.

**Register** (`register_file`): Idempotent file registration with partition values and column stats. Used by the event flush pipeline.

### Integration Points

| Crate | Usage |
|-------|-------|
| `brightflow-api` | Holds `ParquetStore` in `AppState`. Scans tables with date filters for web analytics queries. Loads column semantics on startup. |
| `brightflow-ingest` | `FlushTask` background job: SQLite event buffer -> Parquet files -> `register_file()` with `web:{source_id}` |
| `brightflow-scheduler` | Connector syncs call `merge_parquet()` per endpoint with PKs. Source: `connector:{connector_id}` |
| `brightflow-cli` | Commands: list, info, ingest, export, delete. Event migration. |
| `brightflow-insights` | Uses column semantics and table settings for analysis. Merges inferred + user-overridden schemas. |

### What Litehouse Has Today

- Table and file tracking with SQLite
- Partition-aware file organization (Hive-style for events)
- File-level and column-level statistics for scan pruning
- Append, overwrite, and upsert (merge) ingestion modes
- Partition compaction
- Semantic layer (column roles, KPIs, table settings)
- Source tracking (`web:*`, `connector:*`)
- Lazy evaluation via Polars

### What Litehouse Does NOT Have Yet

- Snapshots / time travel
- Schema evolution with column IDs
- Delete tracking (delete files or deletion vectors)
- Hidden partitioning with transforms
- Data inlining for small writes
- Sort order specifications
- Vacuum / garbage collection of orphaned files
- Multi-table transactions

---

## 2. Apache Iceberg

### Metadata Architecture (Four Layers)

Iceberg's metadata is organized in a strict hierarchy:

**Catalog -> Metadata File -> Manifest List -> Manifest File -> Data Files**

**Catalog**: Maps table names to the current metadata file location. The *only* mutable piece -- everything below is immutable. Can be backed by Hive Metastore, AWS Glue, REST catalog, JDBC, or Nessie.

**Metadata File (JSON)**: Each commit produces an immutable `<version>-<uuid>.metadata.json`. Contains:
- `format-version` (v1, v2; v3 emerging with deletion vectors)
- `table-uuid`, `location` (base URI)
- `schemas` -- list of all historical schema objects, each with a `schema-id`
- `current-schema-id`
- `partition-specs` -- list of all historical partition specifications
- `default-spec-id`
- `sort-orders`
- `snapshots` -- list of valid snapshots with IDs, timestamps, manifest list pointers
- `current-snapshot-id`
- `snapshot-log`, `metadata-log` (audit trail)
- `last-sequence-number` (monotonic write ordering)
- `statistics` (optional: Puffin files with NDV, Theta sketches)

**Manifest List (Avro)**: Each snapshot points to one manifest list. Contains entries for each manifest file, plus partition range summaries enabling **manifest-level pruning** -- skip entire manifests without opening them.

**Manifest Files (Avro)**: Each tracks a set of data files. Per data file:
- File path, format, partition tuple, record count, file size
- Column-level statistics: `null_count`, `nan_count`, `lower_bound`, `upper_bound` (serialized as binary)
- `data_sequence_number`, `file_sequence_number` for ordering
- Status: added (1) or deleted (2), plus snapshot ID

### Schema Evolution

Iceberg assigns a **permanent unique integer ID** to every column at creation. IDs are never reused.

- **Add**: New unique ID; old files return null for this column
- **Drop**: Column removed from current schema; ID preserved for time travel
- **Rename**: Name mapping changes, ID stays the same
- **Type promotion**: Widening (int -> long, float -> double) is metadata-only

Parquet files embed field IDs, so readers map by ID regardless of name or position.

### Partition Evolution

Uses **hidden partitioning** with transforms derived from source columns:
- `identity(col)`, `bucket(N, col)` (murmur3 hash), `truncate(W, col)`
- `year(ts)`, `month(ts)`, `day(ts)`, `hour(ts)`

Partition specs are versioned. Changing the spec affects only new data -- no rewrite needed. Each manifest records its partition spec, so query planners handle mixed specs transparently.

### Delete Mechanisms

- **Positional deletes** (v2): Parquet files with `(file_path, pos)` tuples
- **Equality deletes** (v2): Parquet files with column values (ANDed)
- **Deletion vectors** (v3): Bitmaps in Puffin files -- much more efficient

Delete files apply when the data file's `data_sequence_number` < the delete file's.

### Time Travel

Queries specify a snapshot ID or timestamp. The engine resolves the snapshot, follows its manifest list, reads only visible files. All metadata/data files are immutable, so time travel is free.

### Key Takeaways for Litehouse

- The four-layer metadata hierarchy is powerful but complex. For single-binary deployment, this indirection is unnecessary.
- Column IDs for schema evolution is essential -- adopt this.
- Hidden partitioning with transforms is elegant -- consider for future Litehouse.
- The immutable file model with snapshot-based visibility is the core pattern all formats share.

---

## 3. Delta Lake

### The `_delta_log` Directory

All metadata in a `_delta_log/` subdirectory alongside data:
- **JSON commit files**: Numbered sequentially (`000000000000000000.json`, etc.). Each records one atomic transaction.
- **Checkpoint files**: Parquet files every 10 commits aggregating complete table state.
- **`_last_checkpoint`**: Points to latest checkpoint.

### Commit File Actions

- **`add`**: New Parquet file with path, size, partition values, per-column stats (`minValues`, `maxValues`, `numRecords`, `nullCount`)
- **`remove`**: Logically deletes a file (timestamp + `dataChange` flag)
- **`metaData`**: Schema definition, partition columns, config
- **`protocol`**: Minimum reader/writer protocol versions
- **`commitInfo`**: Operation type (INSERT, DELETE, MERGE), metrics, client version
- **`txn`**: Application-specific transaction IDs for idempotent writes
- **`domainMetadata`**: Extension metadata (v7+)

### Optimistic Concurrency

Non-locking MVCC:
1. Writer writes Parquet files without holding any lock
2. Attempts to write the next sequential commit file (e.g., `000000000005.json`)
3. If another writer claimed that version, checks for true conflicts
4. No conflict: retry with next version. True conflict: abort (orphaned files cleaned by VACUUM)

On cloud stores (S3), requires additional coordination (DynamoDB locking, S3 conditional put).

### Z-Ordering and Liquid Clustering

**Z-ordering**: Multi-dimensional clustering during OPTIMIZE. Interleaves bits of multiple column values for locality-preserving mapping. Reduces query latency 50-90% on fragmented tables. Requires full data rewrite.

**Liquid Clustering** (Delta 3.0+): Successor that allows redefining clustering keys without rewriting, handles high-cardinality better, is incrementally maintained.

### Delta vs Iceberg

| Aspect | Iceberg | Delta Lake |
|--------|---------|------------|
| Metadata format | JSON + Avro manifests | JSON log + Parquet checkpoints |
| Catalog | External (Hive, REST, etc.) | Self-contained (`_delta_log`) |
| Schema tracking | Column IDs | Column names (position-based) |
| Partition evolution | Metadata-only | Requires rewrite |
| Hidden partitioning | Yes (transforms) | No (explicit partition columns) |
| Multi-table txns | Not supported | Not supported |

### Key Takeaways for Litehouse

- Sequential commit log is simpler than Iceberg's manifest hierarchy.
- Checkpoint compaction (aggregate state every N commits) is a useful pattern if Litehouse adds a commit log.
- Z-ordering/liquid clustering is a future optimization opportunity.
- Delta's self-contained metadata (no external catalog) is closer to Litehouse's philosophy.

---

## 4. Apache Hudi

### Table Types

**Copy-on-Write (CoW)**: All data as Parquet. Every update rewrites the entire file group. Optimized for read-heavy workloads.

**Merge-on-Read (MoR)**: Base files (Parquet) + delta log files (Avro). Updates append to logs; compaction periodically merges logs into base files. Two query modes:
- *Read-optimized*: Only base Parquet (fast, possibly stale)
- *Snapshot*: Merge base + logs on the fly (fresh, slower)

### Timeline Architecture

The `.hoodie/` timeline records every action with states: **REQUESTED -> INFLIGHT -> COMPLETED** (or FAILED/ROLLED_BACK). Each action is a timestamped "instant."

**Hudi 1.0 LSM Timeline**: Re-architected to use LSM tree layout. Instead of one file per action (small-file problem), metadata is compacted into fewer, larger files. 10K action reads: ~32ms. Even 10M actions: ~162s.

### Concurrency Control

Table-level or file-group-level locking. Hudi 1.0 introduces **Non-Blocking Concurrency Control (NBCC)** via the LSM timeline, where MVCC guarantees readers always see consistent snapshots isolated from writes.

### Key Takeaways for Litehouse

- The CoW vs MoR distinction is relevant: Litehouse currently does CoW (full rewrite on merge). MoR with delta logs could improve write performance for frequent small updates.
- The timeline state machine (requested/inflight/completed) is useful for tracking long-running operations.
- LSM-based metadata compaction solves the small-metadata-file problem.

---

## 5. DuckLake

DuckLake is the most relevant reference for Litehouse. Released experimentally in May 2025, it reached **v1.0 production-readiness in April 2026**.

### Core Insight

Iceberg and Delta encode metadata in files (JSON, Avro, Parquet) on object storage, then still need a database (catalog) to find those files. DuckLake eliminates the indirection: **store all metadata directly in a SQL database**.

Three-layer separation: **Storage** (Parquet files) + **Compute** (DuckDB or other engines) + **Metadata** (any ACID-compliant SQL database).

### Catalog Database Options

SQLite, PostgreSQL, DuckDB, or MySQL. The only requirement: SQL, primary keys, persistent tables. **SQLite is Litehouse's direct parallel.**

### Complete Metadata Schema (28 Tables)

#### Snapshot Tracking

```sql
ducklake_snapshot (
  snapshot_id PK, snapshot_time, schema_version,
  next_catalog_id, next_file_id
)

ducklake_snapshot_changes (
  snapshot_id PK, changes_made, author,
  commit_message, commit_extra_info
)
```

#### Schema Definition

```sql
ducklake_schema (
  schema_id PK, schema_uuid,
  begin_snapshot, end_snapshot,
  schema_name, path, path_is_relative
)

ducklake_table (
  table_id, table_uuid,
  begin_snapshot, end_snapshot,
  schema_id, table_name, path, path_is_relative
)

ducklake_column (
  column_id, begin_snapshot, end_snapshot,
  table_id, column_order, column_name, column_type,
  initial_default, default_value, nulls_allowed,
  parent_column, default_value_type, default_value_dialect
)
```

#### Data File Management

```sql
ducklake_data_file (
  data_file_id PK, table_id,
  begin_snapshot, end_snapshot,
  file_order, path, path_is_relative,
  file_format, record_count, file_size_bytes, footer_size,
  row_id_start, partition_id, encryption_key,
  mapping_id, partial_max
)

ducklake_delete_file (
  delete_file_id PK, table_id,
  begin_snapshot, end_snapshot,
  data_file_id, path, path_is_relative,
  format, delete_count, file_size_bytes, footer_size,
  encryption_key, partial_max
)

ducklake_files_scheduled_for_deletion (
  data_file_id, path, path_is_relative, schedule_start
)

ducklake_inlined_data_tables (
  table_id, table_name, schema_version
)
```

#### Statistics (Three Levels)

```sql
ducklake_table_stats (
  table_id, record_count, next_row_id, file_size_bytes
)

ducklake_table_column_stats (
  table_id, column_id, contains_null, contains_nan,
  min_value, max_value, extra_stats
)

ducklake_file_column_stats (
  data_file_id, table_id, column_id,
  column_size_bytes, value_count, null_count,
  min_value, max_value, contains_nan, extra_stats
)
```

#### Partitioning

```sql
ducklake_partition_info (
  partition_id, table_id,
  begin_snapshot, end_snapshot
)

ducklake_partition_column (
  partition_id, table_id,
  partition_key_index, column_id, transform
)

ducklake_file_partition_value (
  data_file_id, table_id,
  partition_key_index, partition_value
)
```

#### Sort Orders

```sql
ducklake_sort_info (
  sort_id, table_id, begin_snapshot, end_snapshot
)

ducklake_sort_expression (
  sort_id, table_id, sort_key_index,
  expression, dialect, sort_direction, null_order
)
```

Plus tables for column mapping, SQL macros, tags, and metadata versioning.

### Key Design Decisions

**Temporal validity (`begin_snapshot`/`end_snapshot`)**: Nearly every entity has these fields. An entity is visible in snapshot S if `begin_snapshot <= S AND (end_snapshot IS NULL OR S < end_snapshot)`. This achieves time travel and schema evolution without copying data.

**Data inlining**: For operations affecting <= 10 rows (configurable `DATA_INLINING_ROW_LIMIT`), DuckLake stores data directly in the catalog database. Flushed to Parquet via explicit `CHECKPOINT`. **This eliminates the small-file problem for small transactions.**

**Lightweight snapshots**: A snapshot is just a few rows in `ducklake_snapshot`. DuckLake can manage millions of snapshots because they reference parts of Parquet files, not whole files.

**Transaction model**: All metadata changes execute in a single SQL transaction. Critical path: (1) write Parquet file, (2) single SQL transaction to insert file reference + update stats + create snapshot. Dramatically simpler than Iceberg's manifest approach.

**Delete files**: Separate `-delete.parquet` files with row identifiers. v1.0 also supports Puffin-based deletion vectors (Iceberg v3 compatible).

**v1.0 performance features**:
- `COUNT(*)` from metadata only (8x-258x speedup)
- Dynamic filter file pruning on min/max statistics
- Sorted tables with `SET SORTED BY` for file-level pruning
- Bucket partitioning (murmur3, Iceberg-compatible)

**Planned v2.0**: Git-like data branching, permission-based role controls, incremental materialized views.

### DuckLake vs File-Based Formats

| Advantage | Why |
|-----------|-----|
| Single query for file resolution | One SQL query replaces Iceberg's multi-hop metadata traversal |
| No eventual consistency issues | Database handles all consistency |
| Multi-table ACID transactions | SQL database provides this natively |
| No manifest compaction | No accumulating manifest files |
| Metadata-only migration from Iceberg | Import Iceberg metadata without copying data files |

**Trade-off**: Centralized catalog database. Bottleneck at petabyte scale with thousands of concurrent nodes. **Pure advantage for single-binary/embedded deployments.**

---

## 6. The Post-Modern Data Stack

### The Collapse of the Modern Data Stack

The modern data stack (MDS) was built on composability: separate tools for ingestion, transformation, orchestration, storage, compute, BI, governance, observability, and catalog. By 2025, organizations managed 10-20+ tools with problems:
- **Integration tax**: Most engineering time connecting tools, not building features
- **Metadata fragmentation**: Lineage and governance scattered across systems
- **Cost explosion**: Compounding SaaS pricing
- **Operational complexity**: Specialists needed for each layer

### The Consolidation (2025-2026)

Two directions:
1. **Vendor consolidation**: Large platforms (Snowflake, Databricks, BigQuery) absorbing adjacent capabilities
2. **Embedded/local-first alternatives**: DuckDB, Polars, DataFusion, and tools built on them that collapse the entire stack into a program + files

The 2026 framework: **Keep** core platforms. **Kill** redundant tools. **Combine** overlapping functions.

### The Litehouse Vision

For organizations with data that fits on a single node (the vast majority -- even terabyte-scale is manageable with modern hardware), the entire data platform can be:

- **A single binary** -- no server management
- **Files on disk** -- no cloud storage lock-in
- **SQL interface** -- no new query languages
- **Local-first** -- no network latency for development

This is not anti-cloud -- it is right-sizing. The same binary can scale to cloud object storage when needed. The "post-modern" insight: most organizations don't need distributed systems for their data.

---

## 7. SQLite as a Metadata Catalog

### Precedents

- **DuckLake with SQLite**: First-class support validates this pattern
- **Chroma (vector DB)**: Uses SQLite for metadata in local persistent mode
- **Turso/libSQL, Cloudflare D1, Fly.io LiteFS**: Demonstrate SQLite's viability for production edge/embedded workloads (2024-2025)

### Why SQLite Works for Lakehouse Metadata

1. **Single-file database**: Easy to backup, copy, embed
2. **ACID transactions**: WAL mode for concurrent readers with serialized writes
3. **Zero configuration**: No server process
4. **Proven reliability**: Billions of deployments
5. **Performance**: Metadata operations are small reads/writes -- SQLite excels here
6. **Portability**: Runs everywhere Rust compiles

### Design Considerations

The catalog should track:
- Tables and columns with **unique IDs** (Iceberg-style) and **begin/end snapshot visibility** (DuckLake-style)
- Data files with path, format, record count, file size, footer size
- Per-file column statistics: min, max, null_count, nan flag (TEXT serialization)
- Partition specs and per-file partition values with transforms
- Snapshots: monotonically increasing IDs with timestamps and descriptions
- Delete tracking: delete files or deletion vector references

DuckLake's 28-table schema is an excellent starting template.

---

## 8. Polars as a Query Engine

### Architecture

Polars is organized into 20+ specialized Rust crates:
- `polars-core` -- Series, DataFrame
- `polars-plan` -- logical plan, query optimizer, Hive partition discovery
- `polars-io` -- I/O orchestration, predicate evaluation
- `polars-parquet` -- low-level Parquet encoding/decoding

### LazyFrame Optimization

The optimizer applies:
1. **Predicate pushdown**: Filters pushed to scan level
2. **Projection pushdown**: Only needed columns read
3. **Slice pushdown**: Row limits at source
4. **Common subexpression elimination**
5. **Type coercion optimization**

### Parquet Reading Pipeline

**Two-step read**:
1. Read Parquet footer (metadata) -- small even for multi-GB files
2. Use predicates against row group statistics to skip row groups entirely

**Statistics-based pruning**: For each column/row group, extracts min/max/null_count. Constructs a "Statistics DataFrame" where each row = one row group's stats. Filter evaluated against this DF. Row groups where filter provably returns false are skipped.

**Specialized predicates**: `Equal(Scalar)`, `Between(Scalar, Scalar)`, `EqualOneOf(Box<[Scalar]>)`, `StartsWith(Box<[u8]>)` -- optimized for page-level efficiency.

### Parallel Execution

- **RowGroups**: Parallelize across row groups (many row groups)
- **Columns**: Parallelize within a row group (wide files)
- **Prefiltered**: Evaluate predicates first to build bitmask, then parallelize across columns + row groups with mask

### Hive Partitioning

Polars parses `key=value` from file paths. `materialize_hive_partitions` appends partition values as columns (broadcast efficiently). Can use Hive partition values for file-level predicate pushdown.

### Role in Litehouse

Polars provides the query engine. Its strengths:
- Pure Rust, no JVM
- Native Parquet reader with statistics-based pruning
- Hive partitioning out of the box
- LazyFrame optimizer reduces unnecessary I/O
- Arrow-compatible memory format

Polars does NOT have a built-in catalog or transaction layer -- **this is exactly the gap Litehouse fills with SQLite**.

---

## 9. Adjacent Projects

### delta-rs (Rust Delta Lake)

Native Rust implementation of the Delta Lake protocol. No Java/Spark/Hadoop. Uses Arrow for memory, DataFusion for queries. `LogStore` abstraction handles commit coordination. Python bindings via PyO3. Demonstrates that implementing a lakehouse protocol in Rust is viable and that the Rust data ecosystem (Arrow, Parquet, DataFusion) provides solid foundations.

### hudi-rs (Rust Apache Hudi)

Native Rust implementation (v0.3.0). Uses Arrow. Supports reading Hudi tables without Java/Spark. Integrates with DataFusion, DuckDB, Polars, Daft.

### Lance / LanceDB

**Lance format**: Modern columnar format optimized for ML workloads. Claims 100x faster random access than Parquet. Built for vectors, documents, images.

**LanceDB**: Open-source embedded vector database on Lance. Rust core with Python/Node/Rust SDKs. In-process. Vector similarity + full-text + SQL. **Model of embedded Rust + columnar format.**

### Seafowl

Single-binary analytical database built in Rust with DataFusion + delta-rs. Designed for CDN-friendly HTTP query caching. Splitgraph (creators) acquired by EnterpriseDB. Validates the single-binary pattern.

### DuckDB

Definitive embedded OLAP database:
- Single-file storage, in-process, zero dependencies
- Vectorized columnar execution (SIMD)
- Direct Parquet querying without import
- Single-writer concurrency (simplifies architecture)
- Stable on-disk format since v1.0 (mid-2024)
- Extension system (Iceberg, DuckLake, etc.)

Validates the market for embedded analytics. Proves single-binary deployments handle serious workloads.

### MotherDuck

Cloud service on DuckDB with "hybrid execution": queries split between local DuckDB and cloud "Ducklings." Demonstrates embedded engines can scale to cloud.

### Apache DataFusion

Extensible SQL query engine library in Rust using Arrow. Not a database -- provides planning, optimization, and execution that developers embed. Foundation for Seafowl, GlareDB, Spice AI, InfluxDB 3.0. Native Parquet/CSV/JSON, cost-based optimizer, vectorized execution.

### Other Notable Projects

- **GlareDB**: SQL across databases and data lakes (DataFusion-based)
- **Spice AI**: SQL federation + acceleration + AI inference (DataFusion-based)
- **sqlite-vec**: SQLite extension for vector search
- **VectorLite**: Embedded vector database on SQLite

---

## 10. Key Lakehouse Design Patterns

### Catalog Design: What Metadata to Track

Based on all three major formats plus DuckLake:

**Essential:**
- Tables with unique IDs and versioned schemas (column IDs for safe evolution)
- Data files: path, format, record count, file size, footer size
- Per-file column statistics: min, max, null_count (predicate pushdown)
- Partition specs and per-file partition values
- Snapshots with timestamps (the commit log)
- Delete tracking (delete files or deletion vectors)

**Recommended (from DuckLake):**
- `begin_snapshot`/`end_snapshot` temporal validity on all entities
- Table-level aggregate statistics (total record count, total size)
- Column mapping for schema evolution (unique column IDs)
- Sort order specifications
- Data inlining for small transactions

### Snapshot Isolation and Time Travel

Implementation pattern (from DuckLake):
- Every entity has `begin_snapshot` and `end_snapshot`
- Query at snapshot S: `WHERE begin_snapshot <= S AND (end_snapshot IS NULL OR S < end_snapshot)`
- Snapshots are cheap rows in SQLite
- Time travel = query with a historical snapshot ID
- Current state = `end_snapshot IS NULL`

Since SQLite provides serialized writes (WAL mode), the snapshot model is straightforward. Each write transaction increments the snapshot counter and atomically inserts all metadata changes.

### Schema Evolution

Assign unique integer IDs to columns at creation. Store as `(column_id, column_name, column_type, column_order, begin_snapshot, end_snapshot)`. All Parquet files embed field IDs. Readers map by ID, not name/position.

Operations (all metadata-only):
- **Add**: New column_id; readers return null for old files
- **Drop**: Set `end_snapshot`
- **Rename**: New record with same column_id, different name, new begin_snapshot
- **Reorder**: Update column_order values
- **Type promotion**: Update column_type

### Partition Strategies

1. **Hive-style directory partitioning**: Simple, Polars supports natively
2. **Hidden partitioning with transforms** (Iceberg model): More flexible, store transform + source column in metadata
3. **Bucket partitioning** (murmur3 hash): Good for high-cardinality columns

Recommendation: Start with Hive-style (Polars compatibility), add hidden partitioning later.

### Compaction

**The small file problem**: Streaming/frequent writes create many tiny Parquet files. Each has I/O overhead. Target: 256 MB to 1 GB per file.

**Strategy:**
1. Track file sizes in catalog
2. Background compaction: merge files in same partition where `sum(file_size) < target_size`
3. Write merged file, create new snapshot, mark old files deleted
4. `VACUUM` to physically delete past retention period

**Data inlining** (DuckLake): For small inserts, store directly in SQLite, flush to Parquet periodically. Avoids tiny Parquet files entirely.

### ACID Transactions on Files

The fundamental pattern (all formats share this):
1. Files are immutable -- never modify Parquet in place
2. Write new data files first (before committing metadata)
3. Commit metadata atomically (SQL transaction for SQLite-based catalogs)
4. Failed writes leave orphans cleaned by vacuum
5. Readers see consistent snapshots at all times

For Litehouse with SQLite: single SQLite transaction = one commit. Serialized writes = no concurrent write conflicts. **Significant simplification over Iceberg/Delta/Hudi's optimistic concurrency.**

---

## 11. Recommendations for Litehouse

### Architecture Summary

Litehouse's current foundation (SQLite + Parquet + Polars) is well-validated by the broader ecosystem, especially DuckLake. The recommended evolution path:

### Phase 1: Snapshot Foundation

Add snapshot-based MVMC modeled on DuckLake's temporal validity pattern:

- Add `snapshots` table (snapshot_id, created_at, description, author)
- Add `begin_snapshot`/`end_snapshot` to `tables`, `table_files`, and `column_semantics`
- Each write operation creates a new snapshot in a single SQLite transaction
- Current state queries add `WHERE end_snapshot IS NULL`
- Time travel queries use `WHERE begin_snapshot <= S AND (end_snapshot IS NULL OR S < end_snapshot)`

### Phase 2: Schema Evolution with Column IDs

Replace name-based column tracking with ID-based:

- Add `columns` table with `column_id` (monotonic integer), `column_name`, `column_type`, `column_order`, `begin_snapshot`, `end_snapshot`
- Write field IDs into Parquet files via Polars/parquet-rs metadata
- Readers map by ID, enabling safe renames and reorders

### Phase 3: Data Inlining

Eliminate the small-file problem for frequent small writes:

- Add `inlined_data` table for storing small DataFrames directly in SQLite (as Arrow IPC or JSON)
- Configure row threshold (e.g., 10-100 rows)
- Flush to Parquet via explicit compaction or threshold
- Particularly valuable for event ingestion and connector syncs

### Phase 4: Delete Tracking

Support efficient deletes without full-file rewrites:

- Add `delete_files` table (similar to DuckLake's `ducklake_delete_file`)
- Delete operations create separate files identifying deleted rows
- Compaction merges deletes into base files
- Reduces cost of UPDATE/DELETE on large tables

### Phase 5: Hidden Partitioning

Move beyond Hive-style directory partitioning:

- Add `partition_specs` and `partition_columns` tables with transform types
- Support transforms: identity, bucket(N), truncate(W), year/month/day/hour
- Partition values computed at write time, stored in catalog
- Query planner uses catalog for partition pruning (already partially implemented)

### Phase 6: Advanced Optimizations

- **Sort order specifications**: Track sorted columns for file-level range pruning
- **Vacuum/GC**: Background cleanup of files past retention
- **Statistics refresh**: Recompute table-level stats from file-level stats
- **Metadata-only queries**: `COUNT(*)`, `MIN/MAX` from catalog when possible

### DuckLake Schema as Reference

DuckLake's 28-table schema is the closest reference architecture. Key differences for Litehouse:
- Litehouse uses Polars (not DuckDB) as the query engine
- Litehouse has a semantic layer (column_semantics, table_analysis_settings) that DuckLake doesn't
- Litehouse's source tracking (`web:*`, `connector:*`) is application-specific
- Litehouse can adopt DuckLake's patterns incrementally, not all at once

### The Core Thesis

Litehouse validates the same thesis as DuckLake: for the vast majority of data workloads, the "modern data stack" is over-engineered. A single binary with SQLite (catalog) + Parquet (storage) + a fast query engine (Polars) provides ACID transactions, time travel, schema evolution, and analytical performance -- without a single external dependency.

The post-modern data stack is not a step backward. It is right-sizing the architecture to the actual problem.

---

## Sources

- [Apache Iceberg Specification](https://iceberg.apache.org/spec/)
- [Apache Iceberg Metadata Explained](https://olake.io/blog/2025/10/03/iceberg-metadata/)
- [Understanding Iceberg Delete Files](https://dev.to/alexmercedcoder/understanding-the-apache-iceberg-delete-files-3abo)
- [Iceberg Partitioning Deep Dive](https://amdatalakehouse.substack.com/p/partitioning-with-apache-iceberg)
- [Delta Lake Transaction Log (Databricks)](https://www.databricks.com/blog/2019/08/21/diving-into-delta-lake-unpacking-the-transaction-log.html)
- [Delta Lake ACID Transactions (delta-rs)](https://delta-io.github.io/delta-rs/how-delta-lake-works/delta-lake-acid-transactions/)
- [How Delta Lake Works (Junaid Effendi)](https://www.junaideffendi.com/p/how-delta-lake-works)
- [Hudi LSM Timeline](https://hudi.apache.org/blog/2025/05/29/lsm-timeline/)
- [Hudi Concurrency Control](https://hudi.apache.org/blog/2025/01/28/concurrency-control/)
- [DuckLake: SQL as a Lakehouse Format](https://ducklake.select/2025/05/27/ducklake-01/)
- [DuckLake v1.0 Production Release](https://ducklake.select/2026/04/13/ducklake-10/)
- [DuckLake Metadata Table Specification](https://ducklake.select/docs/stable/specification/tables/overview)
- [DuckLake & Iceberg Architecture Comparison](https://sanj.dev/post/ducklake-iceberg-modern-lakehouse-architecture-2025)
- [Getting Started with DuckLake (MotherDuck)](https://motherduck.com/blog/getting-started-ducklake-table-format/)
- [Post-Modern Data Stack (Matillion)](https://www.matillion.com/blog/comparing-the-modern-data-stack-and-the-postmodern-data-stack)
- [Modern Data Stack is Dead (2025)](https://zionclouds.com/popular-blogs/the-modern-data-stack-is-dead-what-replaced-it-in-2025)
- [2026 Data Product Stack](https://www.ellie.ai/blogs/the-2026-data-product-stack-what-to-keep-kill-and-combine)
- [Polars Predicate Pushdown](https://pola.rs/posts/predicate-pushdown-query-optimizer/)
- [Polars Parquet I/O and Hive Partitioning](https://deepwiki.com/pola-rs/polars/7.3-parquet-io-and-hive-partitioning)
- [Optimising Parquet Reads with Polars](https://www.rhosignal.com/posts/parquet-query-optimisations/)
- [delta-rs GitHub](https://github.com/delta-io/delta-rs)
- [delta-kernel-rs GitHub](https://github.com/delta-io/delta-kernel-rs)
- [LanceDB GitHub](https://github.com/lancedb/lancedb)
- [Seafowl GitHub](https://github.com/splitgraph/seafowl)
- [Apache DataFusion](https://datafusion.apache.org/user-guide/introduction.html)
- [DuckDB Architecture](https://thinhdanggroup.github.io/duckdb/)
- [MotherDuck Architecture](https://motherduck.com/docs/concepts/architecture-and-capabilities/)
- [Schema Evolution in Data Lakehouses](https://risingwave.com/glossary/schema-evolution-in-data-lakehouses/)
- [Lakehouse Format Comparison (Onehouse)](https://www.onehouse.ai/blog/apache-hudi-vs-delta-lake-vs-apache-iceberg-lakehouse-feature-comparison)
- [Small File Problem & Compaction](https://uplatz.com/blog/compaction-strategies-and-the-small-file-problem-in-object-storage-a-comprehensive-analysis-of-query-performance-optimization/)
- [Parquet File Anatomy](https://dev.to/databro/apache-parquet-file-anatomy-row-groups-column-chunks-pages-and-metadata-explained-4ebg)
- [Ultimate Guide to Data Lakehouses (2025-2026)](https://dev.to/alexmercedcoder/the-2025-2026-ultimate-guide-to-the-data-lakehouse-and-the-data-lakehouse-ecosystem-dig)
- [SQLite Edge Production 2026](https://byteiota.com/sqlite-edge-production-2026-database-renaissance/)

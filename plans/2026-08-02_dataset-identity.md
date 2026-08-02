# Source-scoped dataset identity

**Date:** 2026-08-02
**Status:** implemented

## The bug

`DatasetManager::add_dataset` keys store tables as `format!("store:{table_name}")`
(`analytics/session.rs:162`). `source_id` is used to resolve the parquet paths in
`AppState::load_table` and is then thrown away — `DatasetSource::StoreTable` has no
`source_id` field at all.

Consequences:

- Two tabs on different sources with the same table name: tab B's load evicts and
  replaces `store:issues`; tab A keeps sending `datasetId: "store:issues"`
  (`useWsQuery.ts:81`) and silently receives source B's rows. No error, no warning —
  just the wrong data.
- `AppState::load_all_store_tables` (`state.rs:500`) iterates every table across every
  source and reports each as `Ok` while same-named tables clobber one another.

## Two enabling facts found while tracing

- The `unload_store_tables()` call at the top of `load_table` is **vestigial**. Store
  tables hold `DatasetData::Parquet { files: Vec<PathBuf> }` — paths, scanned lazily per
  query. Only `DatasetData::Uploaded(DataFrame)` is materialized, and uploads are never
  unloaded. So the unload frees a path vector. Correct keying and multi-tab support are
  not in tension.
- The correct pattern already exists: `insights::handlers::resolve_dataset`
  (`insights/handlers.rs:486`) takes `source_id` as a separate argument and re-resolves
  paths from the store. The insights path is right; the analytics/WS path is wrong.

## Changes

1. `analytics/session.rs` — added `source_id: String` to `DatasetSource::StoreTable`;
   the id is now `format!("store:{source_id}|{table_name}")`. The `|` separator matches
   the existing `cache_key()` helper (`state.rs:524`) so composite keys read
   consistently across the codebase.
2. `state.rs` — threaded `source_id` into the variant in both `load_table` and
   `load_store_table`. Deleted the `unload_store_tables()` call from `load_table`; the
   function itself stays (public API; no remaining in-tree caller, noted in its doc
   comment).
3. `insights/handlers.rs` — `resolve_dataset` previously did `strip_prefix("store:")` to
   recover a table name, which is the lossy round-trip. It now parses both halves and
   prefers the explicitly-passed `source_id` argument; the parsed source is only a
   fallback.
4. `state.rs` — the `id.starts_with("store:")` filter in `unload_store_tables` still
   matches the new key shape unchanged. Confirmed, left as-is.

## Tests

Co-located `#[cfg(test)] mod tests` per the repo convention:

- `session.rs` — key format for each `DatasetSource` variant.
- Regression: register `src-a/issues` and `src-b/issues`, assert both ids resolve **and
  resolve to different file sets**. Confirmed failing against the pre-fix code (both
  registered under `store:issues`, second clobbered the first) before the fix landed.
- `insights/handlers.rs` — `parse_dataset_ref` round-trip: composite id, bare table
  name, and the precedence rule that an explicit `source_id` wins over a parsed one.

## Deliberately out of scope (follow-up)

Retire `DatasetManager` for store tables entirely by adding `sourceId` to the
ts-rs-exported `Query` struct and resolving paths per query exactly as `resolve_dataset`
does. That eliminates the bug *class* and collapses two divergent code paths, but it
changes the WebSocket message shape and touches `useWsQuery.ts` plus generated types —
it deserves its own review rather than riding along with a correctness fix.

## Verification

- `cargo test -p brightflow-api`
- Manual: load a table under source A in one tab, a same-named table under source B in
  another, confirm both tabs query their own data.

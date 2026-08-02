# Implementation Plan: Test Backfill — First Wave (revised)

**Research context:** Continues
[`reports/2026-07-25_codebase-assessment.md`](../reports/2026-07-25_codebase-assessment.md)
(§3.2 frontend zero tests, §3.3 uneven coverage, §6.4 frontend test foundation,
§6.5 scheduler/store tests) and builds on
[`plans/2026-08-02_two-patterns-foundation.md`](./2026-08-02_two-patterns-foundation.md)
(conventions codified, `vp test` wired, `clippy.toml` `allow-*-in-tests`).

---

## Context

The foundation plan landed one seed test per side. This is the first real
backfill wave on the pure-logic surfaces §6.5 names as highest-risk: the two
crates that run **unattended** (scheduler, store), where a silent logic bug
corrupts data with nobody watching, plus the frontend's query-builder logic.

Verified baseline (measured, not assumed): **406 Rust tests** — engine 289,
api 89, store 17, llm 11, and **scheduler/cli/connect/core at exactly 0**.
Frontend: **1 test file, 16 tests**. `cargo clippy --all-targets --workspace`
is currently clean, and `npm run test` currently passes.

Research turned up three things the draft plan missed, which change its shape:

1. **Nothing runs the frontend tests.** `scripts/pre-commit` runs
   `cargo test --workspace`, but for the frontend only `npx vp staged`
   (types + lint + format). There is **no CI** in the repo at all. Adding
   frontend tests that no automated path executes is decorative.
2. **Two latent bugs sit inside the exact functions being tested.** Writing
   tests as drafted would have enshrined both as "expected behavior."
3. **The env-var test strategy was unsound.** `std::env::set_var` mutates a
   process-global `environ`; unique variable names do *not* prevent the race
   (the draft's stated mitigation), because `setenv` contends on the shared
   table regardless of key.

Intended outcome: scheduler goes 0 → covered on its only pure logic, store
gains its first pure-logic unit tests, the frontend gains real coverage that
actually gates commits, and two real bugs get fixed rather than pinned.

## The two bugs (confirmed by reading the source)

**A. `substitute_env_vars_str` can hang** — `crates/brightflow-scheduler/src/lib.rs:429`.
The `while let Some(start) = result.find("${")` loop re-scans from position 0
after every substitution, so it re-expands substituted *values*. A
self-referential var (`FOO=${FOO}`) never shrinks the string → infinite loop,
hanging the scheduler on a connector sync. Re-expanding env values is also
wrong on its own terms.

**B. `build_pruning_query` silently returns zero files** —
`crates/brightflow-store/src/scan.rs:50`. All partition filters share a single
`fp` join alias, so two `PartitionEq` filters emit
`AND fp.partition_key = ? ... AND fp.partition_key = ?` against **one** joined
row — unsatisfiable. Multi-partition pruning returns an empty file set instead
of the matching files. The adjacent `ColumnRange` code already does this
correctly with per-index `fcs{i}` aliases; partitions just never got the same
treatment.

## Approach

Target only pure logic (no DB, filesystem, network, or DOM), fix the two bugs
in the same wave so the tests pin correct behavior, and close the runner gap so
the tests have teeth. Chosen over a broad "test everything in the zero-coverage
crates" sweep because most of scheduler/store is I/O that belongs in
integration tests — out of scope for the unit-test convention.

## Files

**Modified**
- `scripts/pre-commit` — run frontend unit tests.
- `crates/brightflow-scheduler/src/lib.rs` — bug-A fix + testability refactor + `mod tests`.
- `crates/brightflow-store/src/scan.rs` — bug-B fix + `mod tests`.
- `crates/brightflow-store/src/stats.rs` — `mod tests`.
- `brightflow-app/src/stores/query.ts` — add the missing `/** ... */` module header.
- `CLAUDE.md` — frontend unit-only rule; add the undocumented `brightflow-llm` crate.
- `brightflow-app/CLAUDE.md` — mirror the unit-only rule.

**Created**
- `brightflow-app/src/composables/useOperators.test.ts`
- `brightflow-app/src/stores/query.test.ts`

**Deleted:** none.

## Steps

### 1. Close the runner gap

In `scripts/pre-commit`, inside the existing frontend block (after the
`npx vp staged` check, which is guarded by the same `node_modules` test), add a
test run that sets `FAILED=1` on failure, matching the surrounding style:

```bash
echo -e "${BLUE}[Frontend]${NC} Running unit tests..."
if ! npx vp test --run 2>/dev/null; then
    echo -e "${RED}✗ Frontend tests failed${NC}"
    FAILED=1
else
    echo -e "${GREEN}✓ Frontend tests${NC}"
fi
```

Confirm `--run` is the correct non-watch flag for `vp test` before committing
(Vitest 4 defaults to run-mode when non-TTY, but the hook must not hang under
any circumstance — verify by running it directly).

### 2. Fix bug A + make the substitution unit-testable

In `crates/brightflow-scheduler/src/lib.rs`, extract the pure logic so tests
never touch process env:

```rust
/// Expand `${VAR}` occurrences using `lookup`, scanning left to right.
///
/// The cursor only moves forward, so substituted values are never re-scanned:
/// this both avoids re-expanding env content and makes a self-referential
/// variable (`FOO=${FOO}`) terminate instead of looping forever.
fn substitute_with(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String { ... }

fn substitute_env_vars_str(s: &str) -> String {
    substitute_with(s, |k| std::env::var(k).ok())
}
```

Implement `substitute_with` with a forward cursor: find the next `${` at or
after `cursor`, find its closing `}`, push the literal prefix and the
replacement onto an output `String`, then advance `cursor` past the closing
brace. Unresolved vars still expand to `""` (preserving today's
`unwrap_or_default()` behavior); a `${` with no `}` still passes through
unchanged. Give `substitute_env_vars_str` the `///` doc comment it currently
lacks — the only function in the crate missing one.

Then append `#[cfg(test)] mod tests { use super::*; ... }` covering, via
`substitute_with` with a closure (fully pure, no env, no locking, no races):

- no `${` → unchanged; single var; **two vars in one string** (`"${A}${B}"`,
  pins the offset arithmetic); missing var → `""`.
- malformed `"${nope"` → returned as-is.
- **self-referential value** (`lookup` returns `"${FOO}"` for `"FOO"`) →
  terminates and yields the value literally. This is the bug-A regression test;
  it would hang before the fix.
- `substitute_env_vars_in_json` over a nested object/array/string mix: only
  string leaves containing `${` change; numbers, bools, nulls, and structure
  survive intact.

Keep at most **one** test that touches real env (asserting
`substitute_env_vars_str` wires the lookup through) — or omit it entirely and
rely on `substitute_with` coverage. Do **not** use `std::env::set_var` in a
multithreaded harness.

### 3. Fix bug B + test the pruning query

In `crates/brightflow-store/src/scan.rs`, give each partition filter its own
join alias, mirroring the existing `fcs{i}` pattern:

```rust
for (i, _) in partition_filters.iter().enumerate() {
    let _ = writeln!(sql, "INNER JOIN file_partitions fp{i} ON fp{i}.file_id = tf.id");
}
```

and change the `PartitionEq` / `PartitionRange` arms to emit `fp{i}.partition_key`
/ `fp{i}.partition_value` using the enumerated index. Replace the now-stale
line-49 comment ("use a single join, filter in WHERE") with one explaining why
each partition filter needs its own alias.

Append `#[cfg(test)] mod tests` asserting **structure, not exact SQL text** (so
whitespace refactors don't break tests):

- empty filters → binds are exactly `[table_id]`; no `INNER JOIN`/`LEFT JOIN`.
- one `PartitionEq` → binds `[table_id, key, value]` in that order; one `fp0` join.
- `PartitionRange` with `min`/`max` `None` → those binds omitted; with both →
  present in min-then-max order.
- `ColumnRange` with `None` min/max → binds omitted; `fcs0` alias present.
- **two `PartitionEq` filters → two distinct join aliases (`fp0`, `fp1`) and
  each `partition_key` predicate bound to its own alias.** This is the bug-B
  regression test.
- mixed partition + column filters → bind ordering is table_id, then all
  partition binds, then all column binds.

Assert placeholder counts with `sql.matches('?').count()` and check
`sql.contains("fp1.")` for aliasing.

### 4. Test the stats helpers

Append `#[cfg(test)] mod tests` to `crates/brightflow-store/src/stats.rs`:

- `supports_min_max`: `Int64`/`Float64`/`String`/`Date`/`Datetime(_, _)`/
  `Duration(_)` → `true`. **`Boolean` → `true`** — this is real current
  behavior; pin it deliberately with a comment so a future change to exclude
  bool is a visible, reviewed diff. For the `false` cases use
  **`DataType::List(Box::new(DataType::Int64))` and `DataType::Binary`** — both
  ungated. Avoid `DataType::Struct`: it is behind the `dtype-struct` feature and
  only compiles here transitively via polars' `pivot` feature, an invisible
  coupling that would break confusingly.
- `format_scalar`: `Scalar::null(dtype)` → `None`; `AnyValue::String("a")` →
  `Some("a")` (unquoted — that is the function's whole point);
  `AnyValue::StringOwned(...)` → same; a numeric scalar → `Some` of its
  `Display`. Do **not** enumerate every `AnyValue` variant; that drifts with
  Polars versions and isn't the contract under test.
- Add one test for the public `extract_column_stats` over a small in-memory
  `df!` covering a numeric column, a string column, and a column with nulls.
  This is pure (no fs) and pins the real contract that the two private helpers
  only serve.

Construct scalars via `Scalar::new(DataType::String, AnyValue::String("a"))`
(both `pub`, verified in polars-core 0.48.1).

### 5. Frontend: `useOperators.test.ts`

Pure, no Pinia. Match the seed test's style exactly (see below). Cover:

- `getOperatorsForType`: `'string'` → 6 ops (`eq, ne, isNull, isNotNull,
  contains, in`); `'int'`/`'float'` → 9; `'boolean'` → **4** (no `contains`,
  no numeric ops, and **`in` is excluded** — easy to get wrong).
- normalization: `'varchar'`/`'utf8'` → string; `'BIGINT'` (case-insensitive) →
  int; `null`/`undefined`/`''` → string; unknown (`'date'`) → string fallback.
- `getOperator('eq')` → label `'equals'`; unknown → `null`. Note `noValue` is
  `undefined` (absent), not `false`, on the raw `OperatorDef`.
- `operatorNeedsValue`: `'isNull'` → `false`; `'eq'` → `true`; unknown → `true`.
- `operatorIsArray`: `'in'` → `true`; `'eq'` → `false`; unknown → `false`.
- `getDefaultOperator`: string → `'contains'`; int/float/boolean → `'eq'`.

### 6. Frontend: `query.test.ts`

`import { setActivePinia, createPinia } from 'pinia'` with
`setActivePinia(createPinia())` in a `beforeEach`.

**Critical:** a fresh store is **not** empty. `filter.enabled` and
`limit.enabled` default to `true` with `limit = 100`, so default `operations`
is `[{ type: 'limit', n: 100 }]`. The "disabled sections emit nothing" test
must explicitly disable `limit` to assert `[]`.

Build state by assigning to store refs directly (`store.filters.push({...})`,
`store.sections.groupBy.enabled = true`) rather than calling `addFilter()` —
this gives deterministic ids. (`crypto.randomUUID` *is* available in this Node
25 env, so the draft's "polyfill needed" risk is void; determinism is the
actual reason.) Setup stores auto-unwrap refs — no `.value` in tests.

Cover `operations`: emission order is fixed (filter → groupBy → pivot → select
→ sort → limit); filters with `column: null` or `op: ''` are dropped;
`isNull`/`isNotNull` force `value: null`; `alias` falls back to
`${function}_${column}` via `||` (so `''` → `'count_*'`); pivot emits only when
`values` **and** `columns` are both non-null; sort emits when `sortBy != null`;
limit emits only when `> 0`.

Cover `previewTexts`: `'No filters'` / `'1 filter'` / `'2 filters'` pluralization,
pivot `'Not configured'` when `values == null`, and note that `previewTexts`
**ignores `sections[x].enabled` entirely** — it reflects raw state.

Prefer `toEqual` on whole objects/arrays over indexed property access: strict
TS has `noUncheckedIndexedAccess`, so `ops[0].type` doesn't narrow cleanly.

### 7. Docs

- `CLAUDE.md` → Conventions → Frontend shape, after the `environment: 'node'`
  bullet: tests are **unit only** — no Playwright or other E2E/browser runner;
  a Pinia store is tested in isolation via `setActivePinia(createPinia())`, not
  by mounting the app. Mirror one line in `brightflow-app/CLAUDE.md` `## Testing`.
- `CLAUDE.md` → Project Structure: add the missing
  `crates/brightflow-llm` entry (it's a workspace member with 11 tests but is
  absent from the documented structure).
- `brightflow-app/src/stores/query.ts`: add the `/** ... */` module header that
  convention #1 requires and this file lacks.

## House style to match

**Rust** — follow `crates/brightflow-api/src/ingest/identity.rs` (named
canonical in CLAUDE.md): `#[cfg(test)] mod tests { use super::*; }`,
descriptive snake_case fn names with **no `test_` prefix**, one behavior per
test, no `#[allow]` ceremony. Note the store's existing tests use the `test_`
prefix; follow the canonical style for new modules and don't churn the old ones.

**Frontend** — follow `brightflow-app/src/utils/format.test.ts`: `/** ... */`
file header stating scope *and* what's deliberately out of scope; explicit
`import { describe, test, expect } from 'vitest'`; `test(...)` never `it(...)`;
one `describe` per exported function; **relative** import of the module under
test; `Number.NaN` not bare `NaN`; regex literals need the `u` flag
(`/foo/iu`) for the oxlint unicorn rules.

## Risks & constraints

- **Lint gates are strict.** The hook runs
  `cargo clippy --all-targets --all-features -- -D warnings` — note
  `--all-features`, which the draft omitted; use the exact flags. Every `warn`
  in the workspace lint table is effectively a deny at commit time.
- **`clippy.toml` covers only 7 lints in `#[cfg(test)]`** (`unwrap_used`,
  `expect_used`, `panic`, `indexing_slicing`, `print_*`, `dbg_macro`,
  `useless_vec`). Still live in test modules: `too_many_lines` (threshold **60**
  — a large `mod tests` can trip it), `cognitive_complexity` (15),
  `shadow_reuse`/`shadow_unrelated`. Both target crates carry crate-level
  `#![allow]`s in their `lib.rs` covering most of these, so new test modules
  inherit them — but that's *why* it works, not `clippy.toml` alone. If a lint
  does surface, use `#![expect(clippy::foo, reason = "...")]` per CLAUDE.md.
- **`use super::*` is safe**: `warn-on-all-wildcard-imports` is unset (defaults
  false), which exempts `super::*` from `wildcard_imports`.
- **Frontend test files are linted and type-checked** — `lint.ignorePatterns`
  excludes only `dist/**` and `src/types/generated/**`, with
  `typeAware: true`. Rules that bite: `no-unsafe-type-assertion` (avoid `as`),
  `strict-boolean-expressions`, `eqeqeq`, `no-console`, plus
  `capitalized-comments` and `unicorn/no-useless-undefined`.
- **The `@/` alias is unproven under `vp test`.** `query.ts` imports `@/types`;
  the only existing test uses a relative import, so alias resolution has never
  been exercised. `resolve.tsconfigPaths: true` is top-level and should be
  inherited. **Run `query.test.ts` first** — if it fails it will fail loudly on
  module resolution, and the fix is a `test.alias` entry in `vite.config.ts`
  (config only, still no new deps).
- **Bug-B fix changes generated SQL.** `build_pruning_query` is consumed by the
  catalog scan path; the single-filter output is unchanged in shape (just
  `fp` → `fp0`), and the multi-filter case goes from "always empty" to correct.
  Verify no caller string-matches on the `fp` alias.
- **No new dependencies, either side.** Rust uses `std` + existing `polars`/
  `serde_json`; frontend uses the already-installed `vitest` (4.1.10, via
  `vite-plus`) and `pinia` (a direct dep). Adding a DOM env would require a new
  devDependency — explicitly not needed here.
- **Stores that can't be unit-tested under `environment: 'node'`:** `ui.ts`,
  `source.ts`, and `insightsActivity.ts` touch `localStorage` at store-setup
  time, so instantiating them throws. `query.ts` and `pivot.ts` are the only
  two clean ones. Relevant to any follow-up wave.

## Verification

Run in this order; each must be green before moving on.

```bash
# Frontend
cd brightflow-app
npm run test          # new useOperators + query tests pass; run query.test.ts first
npm run check         # types + lint + format clean on the new files

# Rust
cd ..
cargo test -p brightflow-scheduler --lib   # 0 -> covered; must not hang (bug A)
cargo test -p brightflow-store --lib       # scan + stats + existing
cargo clippy --all-targets --all-features -- -D warnings   # exact hook flags
cargo fmt --all -- --check
cargo test --workspace                     # no cross-crate regression
cargo build --release                      # per CLAUDE.md workflow
```

Then exercise the hook end-to-end, since step 1 modifies it:

```bash
./scripts/install-hooks.sh    # if not already symlinked
bash scripts/pre-commit       # must pass AND must not hang on the frontend test step
```

Sanity-check the bug fixes actually regress-test: temporarily revert each fix
and confirm the corresponding test fails (the self-referential substitution
test should hang/fail; the two-`PartitionEq` test should fail on aliasing).
Restore the fixes afterward.

Expected end state: Rust tests 406 → ~425; frontend 16 → ~50 across 3 files;
scheduler no longer at zero; frontend tests gate commits.

## Out of scope

- **E2E / Playwright / browser tests** — excluded by request; codified in step 7.
- **CI (assessment §6.2)** — the pre-commit hook is the chosen enforcement point
  for now; a GitHub Actions workflow is a separate plan.
- **`stores/connection.ts`** — the five status computeds and the `onMessage`
  subscribe/unsubscribe bookkeeping *are* testable without mocking `WebSocket`,
  but `connect()`/`send()`/`handleMessage` are not. Deferred as a unit.
- **Integration tests** for `ingest_parquet`, `execute_sync`,
  `resolve_topic_config`, `Scheduler::tick` — §6.5's integration surface.
- **Second-wave unit targets identified but deferred** (recorded so the next
  session doesn't re-derive them): store `ingest.rs::schema_to_json` and
  `concat_df` (both pure, load-bearing); frontend `stores/pivot.ts`,
  `components/enrich/promptTokens.ts` (mirrors Rust parsing — real drift risk),
  `components/enrich/promptDiff.ts` (hand-rolled LCS),
  `components/sources/csvPreview.ts` (hand-rolled parser); and the three
  uncovered `format.ts` exports (`formatCompact`, `formatPercent`,
  `displaySegmentValue`).
- **`brightflow-cli` / `connect` / `core`** — no meaningful pure-logic targets.
- **The engine/api backfill** — engine has 289 tests; api's gaps are larger.
- **`docs/supervised_topics.md`** (§6.1), longbow publish (§6.3), embedding
  ceiling (§6.6), insights triage (§6.7) — separate plans.
- Any change to workspace `Cargo.toml` lints or `clippy.toml` — settled in the
  foundation plan.

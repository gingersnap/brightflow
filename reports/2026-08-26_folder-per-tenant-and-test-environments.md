# Folder-per-Tenant Storage and Environment-as-Folder Testing

**Date:** 2026-08-26
**Scope:** Assess the idea that each tenant — and each environment (test, staging,
production) — is a single folder on disk containing its SQLite catalogs + Parquet
payloads, and that a test environment is simply another folder (a "tenant").
**Method:** Static review of this repo (path model, test harness, philosophy docs)
plus web research into how others build this pattern (database-per-tenant,
branch-databases, environment-per-file). No code changed.

---

## 1. The idea, stated precisely

Because Brightflow's data layer is SQLite (catalogs) + Parquet (payloads), the
natural unit of isolation is **the whole workspace folder**, not a single SQLite
file. That unit doubles as the unit of tenancy and the unit of environment:

- a **tenant** is a folder (`data/workspaces/{name}/`);
- a **test/staging/production env** is just another folder;
- the multitenant and the environment-switching problems are the same problem,
  solved by pointing at a different directory.

## 2. What the repo already does

The storage half of this idea is **already the architecture**, and has been since
the `WorkspacePaths` decision:

| Fact | Evidence |
|---|---|
| Every data location derives from one base + one workspace name | `crates/brightflow-core/src/lib.rs` — `{base}/workspaces/{workspace}/{sub}` |
| A workspace folder holds all its state | `auth.db`, `litehouse.db`, `scheduler.db`, `ingest.db`, `events-buffer/` (per-source SQLite), Parquet/connector dirs |
| A throwaway/fresh workspace is anticipated | module doc: "a second workspace — or a throwaway test one — a matter of setting `BRIGHTFLOW_DATA_DIR` and `BRIGHTFLOW_WORKSPACE`" |
| Each SQLite DB migrates itself on open | `auth/db.rs`, `ingest/db.rs`, `store/db/mod.rs`, `scheduler/db.rs` all call `migrate` at pool creation |
| Tests already isolate by throwaway folder | `tempfile::TempDir` + DBs opened inside it (`tests/router_contract.rs`, `store/tests/enrichment_functions.rs`) |

Two gaps against the full idea:

1. **The routing layer does not exist.** `WorkspacePaths::from_env()` is resolved
   once per process; there is no per-request tenant → folder mapping.
   `crates/brightflow-api/src/actions/events.rs` documents the single-workspace
   assumption ("one workspace, so every connected client gets every event").
2. **Testing uses disposable temp dirs, not a named test tenant.** That is right
   for the unit/integration inner loop (see §5 guardrail).

Notably, the abstract `TenantId`/`DatasetId` layer was considered and **deliberately
dropped** in the 2026-08-03 simplification; core was kept as a zero-dep paths crate.
The folder-is-the-tenant instinct is the same no-abstraction philosophy.

## 3. Methodology

Same idea researched in the wider ecosystem. The two closest families:

### 3a. Environments / branches as separate DB files

- **Rails default convention** already ships this: `storage/development.sqlite3`,
  `storage/test.sqlite3`, `storage/production.sqlite3` — each environment is its
  own SQLite file.
- **Fractaled Mind — "Enhancing Rails SQLite Branch Databases"** (Stephen Margheim,
  2023-09-06): names the *development* DB file after the current git branch
  (`storage/<branch>.sqlite3`) and runs `db:prepare` on boot, so switching branches
  auto-switches schema and the DB is always ready. Positioned as PlanetScale's
  database-branching feature for free. This is the closest published analog to the
  environment-as-folder idea.
  https://fractaledmind.com/2023/09/06/enhancing-rails-sqlite-branch-databases/

### 3b. Database-per-tenant in production

- **sqliteproxy / uRadical** — single Go binary; every tenant is a `.db` file in
  `./data/`; per-tenant migrations on first access; context-routing proxy keeps app
  code single-tenant. Production, regulated industries. Reports ~170ns routing
  overhead and calls per-tenant migrations "a superpower." This is the reference
  implementation of both the storage model **and** the routing layer we lack.
  https://uradical.io/latest-news/how-we-build-single-binary-multi-tenant-services
- **Laravel Tenant SQLite** (nexus-scholar) — per-tenant isolated SQLite file with
  predictable lifecycle.
  https://github.com/nexus-scholar/laravel-tenant-sqlite
- **HelperX** — 200 per-"slot" SQLite files in prod, separate global `global.db`
  for auth/billing ~ our `auth.db` + per-workspace DBs. One year in production.
  https://dev.to/helperx/sqlite-in-production-why-we-chose-it-over-postgres-for-a-multi-tenant-saas-44n8
- **Shardine** — per-tenant SQLite for Rails, connection switched per request via
  `SERVER_NAME`; "a can of shardines."
  https://engineered.at/articles/a-can-of-shardines-sqlite-multitenancy-with-rails
- **Turso / libSQL** — the major commercial champion of database-per-tenant;
  argues a DB-as-a-file removes all the classic objections (cost, pooling, backup);
  seeds new tenants from a **template database** (≈ our "seed a test folder from a
  template folder"); case studies like Poke (a DB per user).
  https://turso.tech/blog/multi-tenancy-at-scale

### 3c. Test-oriented and tooling

- **Worker-per-database** parallel test runs (one SQLite file per worker — Vitest
  global setup, Pest) is common practice.
- **"SQLite for tests"** is a well-worn genre; our position is stronger because
  prod is *also* SQLite, giving test/prod parity (the genre's main complaint is
  divergence).
- **Litestream directory replication** and **Fly.io "all-in on server-side
  SQLite"** exist specifically to replicate a directory of per-app/tenant DBs.
  https://litestream.io/guides/directory/
- **Authority**: Richard Hipp ("Appropriate Uses for SQLite") and Kent Beck
  ("the only database most applications will ever need") cited as endorsing
  SQLite-as-the-database broadly.

## 4. Fit with project philosophy

Aligned on all three core axes:

1. **Efficiency.** A tenant is a folder; creation is a µs filesystem op; no server
   process, no socket pooling, no idle-memory reservation. Consistent with "speed
   is the feature."
2. **No unnecessary abstraction.** The folder *is* the tenant; no `tenant_id`
   column, no RLS, no bespoke test harness separate from the real path. Tests reuse
   `WorkspacePaths`/env exactly like prod. Matches the deliberate removal of
   `TenantId`/`DatasetId`.
3. **Real-path coverage.** Because payloads are Parquet, a "test tenant" runs the
   genuine Polars read path over real files, not an in-memory mock — fewer mocks,
   more coverage of what makes the product fast.

## 5. Guardrails / honest tradeoffs

1. **Don't replace the fast inner loop.** Disposable `TempDir` is already "a
   throwaway tenant" and is the efficient choice for cargo-test/vitest unit +
   integration tests. The **named test tenant** (`BRIGHTFLOW_WORKSPACE=test`) is
   the environment layer — the real server under `run-all`, frontend against it,
   bug reproduction, demo/seed data. Keep the two separate.
2. **Don't build the routing layer yet.** True multitenancy needs per-request
   tenant→folder mapping + a registry. That is the easy 10% on top of a store
   model we already have; building it now is precisely the premature abstraction
   the 2026-08-03 plan deleted.
3. **Migration × N.** Each folder self-migrates on open; a fleet means each
   migration runs N times, and a fleet that's behind is state you own.
4. **Inodes + file descriptors.** Many small SQLite files consume inodes and FDs;
   fine for dozens of environments, a consideration before thousands of tenants.
5. **Cross-tenant analytics.** Aggregating across folders means walking many
   folders. Fine for Brightflow (workspace = analytics unit); a future global
   dashboard is a separate warehousing problem, not a reason to share one catalog.
6. **Concurrent writes.** SQLite is single-writer per DB; amortized across tenants
   (serialize within a tenant only), but a hot tenant is still one writer. Use WAL.

## 6. Bottom line

The idea is sound, is the repo's de facto architecture already, and is an
established, actively-championed pattern in the ecosystem (branch databases,
database-per-tenant family). The concrete near-term move is to formalize a **named
test workspace folder** driven by `BRIGHTFLOW_WORKSPACE=test` for the environment
layer, while keeping disposable temp dirs for the unit inner loop, and to hold off
on request-level tenant routing until the product actually serves multiple tenants.

# Implementation Plan: Frontend→Backend Integration Tests (subcutaneous, no E2E)

**Date:** 2026-08-29
**Status:** Proposal — decisions below flagged as *confirmed* are settled with the
author; *default* are assumptions until reviewed (see §8).
**Context:** Continues `../plans/2026-08-29_test-workspace-architecture.md`, which
delivered the committed test template + ephemeral copy mechanism for storage-backed
tests. This plan adds the *next tier up*: drive the frontend's genuine data layer —
the `services/api` client and Pinia Colada (the server-data store) — against a real
backend over a real HTTP socket and a real storage round-trip. Explicitly an
**integration tier below the UI**, not E2E — no browser, no Playwright, no DOM.

---

## 1. The converged design

Two tiers, one runner. Unit tests remain the fast inner loop; a new *integration*
tier exercises the frontend data layer against the real backend on an ephemeral
port backed by the same committed template the backend tests use.

| Tier | What it exercises | Backend | Data |
|---|---|---|---|
| **Unit** (default) | co-located logic + app Pinia stores | none (bypassed) | none |
| **Integration** | `services/api` client + Pinia Colada queries/mutations, UI bypassed | real `build_app` over HTTP on an ephemeral port | fresh copy of `testdata/workspaces/test` |

Key properties:
- One runner (`vp test`); tiers split with Vitest `projects` in the `test:` block
  of `vite.config.ts` — no `vitest.config.ts`, no new runner dependency.
- Real transport (HTTP socket), real session/auth (login as the committed demo
  user), real Parquet/SQLite read + write.
- **Not E2E**: no Vitest `browser` mode, no Playwright, no mounted components, no
  DOM. Assertions are on data round-trips.

## 2. Decisions reached (summary)

- **Confirmed — not E2E.** This is a subcutaneous integration tier; browser/DOM
  is out of scope by definition.
- **Confirmed — the seam is `services/api` + Pinia Colada.** Pinia Colada *is* a
  store (a Pinia-backed server-data store); the app's own `src/stores/*` hold
  client state only and never import `services/api`, so they are not the seam.
- **Confirmed — runtime-overridable API base** (default `VITE_API_BASE`) so tests
  can target an ephemeral port with no build-time reconfiguration.
- **Confirmed — real HTTP on an OS-assigned ephemeral port** (bind :0, report the
  assigned port), and **real session** via replayed login `Set-Cookie`.
- **Confirmed — minimal backend:** the real `build_app` router, but no
  scheduler/WebSocket/timers unless a test targets them.
- **Default — server boot/teardown in Vitest `globalSetup`.**
- **Default — milestone-1 store is `sources`** (read + a persisted-mutation
  round-trip), preceded by a login/session check.

## 3. Design — concrete mechanics

### 3.1 Tier separation (latest Vite+ / Vitest setup)

- All test config lives in the `test:` block of `vite.config.ts`. Split tiers with
  **`test.projects`** there — Vite+ injects its plugins into each project.
- **Unit project:** `include: ['src/**/*.test.ts']`, `environment: 'node'`
  (unchanged; remains the default `npm run test`).
- **Integration project:** `include: ['src/**/*.integration.test.ts']`,
  `environment: 'node'`, plus `globalSetup` that boots/tears down the backend.
  Kept out of the default glob so the unit inner loop stays cheap; runs explicitly
  (filtered via `--project` or a dedicated npm script added later).

### 3.2 Runtime API-base override (the one production-touching change)

`core.ts` fixes `API_BASE = import.meta.env.VITE_API_BASE ?? ''` at module load.
Add a small runtime override read at request time (a module-level `let` + setter
that tests set in `beforeAll` and reset in `afterAll`), defaulting to today's
behavior when unset. Non-breaking; reversible.

### 3.3 Minimal backend for tests (a socket, not in-process)

Backend ITs assemble `AppState::with_store` + `AuthDb` + `build_app` and hit the
router in-process (`oneshot`) — that skips transport. Integration tests need a real
socket:
- A small test-facing server entry that builds state from a fresh
  `brightflow_test_support` copy, binds `build_app`'s router to `127.0.0.1:0`,
  prints the bound port (`local_addr`) to stdout, and serves until signalled (then
  aborts the session sweeper, as `serve` does).
- Played as a subprocess (`cargo build` + run a `[[bin]]`); `globalSetup` spawns it,
  parses the port, and kills it on teardown.

### 3.4 Session / cookie handling

`core.ts` sends `credentials: 'include'`, but Node's `fetch` does not persist
cookies. The integration project uses a tiny fetch wrapper that captures the login
`Set-Cookie` and replays it as `Cookie` on later requests. Zero new deps.

### 3.5 Milestone-1 test set

1. **Login/session check:** log in as the committed demo user through the real
   client; a protected endpoint answers 200 (not 401) — proves the shared
   plumbing (API base + session) every later test depends on.
2. **`sources` round-trip:** read genuine stored data, perform a mutation, re-read
   to assert it persisted in the workspace.

## 4. Building blocks already in place

- Committed template `testdata/workspaces/test` (migrated DBs + a real Parquet
  table) and `brightflow_test_support::copy_template()` for the ephemeral copy.
- Committed demo user for the real login path.
- `build_app` is public (ITs already assemble state+auth+router), and `serve`
  is the precedent for socket binding.

## 5. Keeping it from going stale

- Integration tests are data-backed by the same committed template and
  self-migrate on open, so schema drift rides the existing machinery.
- The unit fast loop must stay cheap: integration is a separate `projects` entry +
  explicit run, never folded into the default unit glob.
- Extend the template only when a fixture genuinely needs new data.

## 6. Guardrails / deliberate non-goals

1. **Not E2E:** no `browser` mode, no Playwright, no component mounting, no DOM.
2. **Minimal server:** no scheduler/WebSocket/background timers unless targeted.
3. Real data only through the genuine paths; never point fixtures at real
   production sources.
4. Unit inner loop stays untouched and cheap.
5. No new test-runner dependency — everything rides `vp test`.

## 7. Proposed phases

- **Phase A — plumbing:** runtime API-base override; minimal socket server entry;
  integration `projects` + `globalSetup` harness; session/cookie wrapper.
- **Phase B — milestone-1 tests:** login/session check + `sources`
  read/write round-trip green.
- **Phase C — widen (follow-on):** more stores/domains; optionally carve the
  integration tier into the pre-commit hook as a separate (non-default) command.

## 8. To confirm before implementation

- **Default:** backend booted by Vitest `globalSetup` (single command) vs a
  separate shell script that starts the server then runs Vitest (easier to reuse
  manually). I lean `globalSetup`, with the boot logic in a reusable module.
- **Default:** add a dedicated test-facing server entry (a `[[bin]]`) that binds
  port 0 and reports it — confirm this is acceptable vs reusing `serve()` (which
  also starts scheduler/ingest/sampler — heavier than needed for these tests).
- **Default:** integration tests co-located as `src/**/*.integration.test.ts` —
  confirm the naming/glob.
- **Default:** milestone-1 store is `sources` — confirm, or name a different seam
  you care about more.
- Confirm the runtime API-base override in `core.ts` — the one production-adjacent
  change — is acceptable as designed.

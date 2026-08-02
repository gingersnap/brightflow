# Brightflow

## Project Overview

Analytics platform with a Rust backend (Axum + Polars) and Vue 3 frontend.

## Tech Stack

- **Backend**: Rust, Axum, Polars, SQLite (Litehouse)
- **Frontend**: Vite+ (Vite 8), Vue 3, TypeScript, Nuxt UI 4, Tailwind CSS 4, Pinia

## Project Structure

- `crates/brightflow-cli` - Binary (run-all, serve, schedule, insights)
- `crates/brightflow-core` - Shared types (TenantId, DatasetId, errors)
- `crates/brightflow-connect` - Data connectors
- `crates/brightflow-store` - SQLite-backed Parquet storage (Litehouse)
- `crates/brightflow-engine` - Analysis engine, NLP primitives, enrichment orchestration
- `crates/brightflow-llm` - Provider-agnostic LLM client (OpenAI chat-completions dialect)
- `crates/brightflow-scheduler` - Background job runner for connector syncs
- `crates/brightflow-api` - HTTP API server (Axum + Polars) with integrated event ingestion
- `brightflow-app/` - Vue 3 frontend (see its CLAUDE.md for detailed style rules and conventions)

## Code Style

- **Rust**: `cargo fmt`, `cargo clippy`, `cargo audit`
- **Frontend**: Vite+ unified toolchain (Oxfmt + Oxlint + tsgolint), strict TypeScript

## Conventions

Two rules every change should follow, lifted from the engine-crate habits to
repo-wide expectations:

1. **Inline comments are the primary documentation.** Modules, non-trivial
   functions, and non-obvious decisions get a comment explaining *why*, not
   *what*. For Rust that means a `//!` module doc on every `src/` file; for the
   frontend a `/** ... */` header on each module. External prose docs are for
   architecture and onboarding, not for explaining individual functions.
2. **Unit tests per piece the agent touches.** When you add or change pure
   logic, add or update a co-located unit test covering the new behavior. Don't
   leave a touched function without coverage; don't add tests for code you
   didn't touch unless that's the explicit task.

### Rust shape

- `//!` module doc comment at the top of every file under `crates/*/src/`.
- Pure-logic files get a `#[cfg(test)] mod tests { use super::*; ... }` block at
  the end, mirroring the function under test (`crates/brightflow-api/src/ingest/identity.rs`
  is the canonical example).
- Test code may use `.unwrap()` / `.expect()` / `panic!` / indexing / `dbg!` /
  `println!` freely — these are allowed under `#[cfg(test)]` via the
  `allow-*-in-tests` keys in `clippy.toml` (production code stays fully denied).
  This matches the team's existing intent: the engine crate already annotates test
  modules the same way. Use `assert!`/`assert_eq!` for assertions; reserve
  `#![expect(clippy::foo, reason = "...")]` at the top of a test module for any
  *non-covered* lint (e.g. `unnecessary_wraps`) that surfaces there — `expect`
  warns if it later becomes unnecessary, so the list doesn't rot.
- Cross-crate / HTTP-level integration tests live in `crates/*/tests/`.
- Run a single crate's unit tests with `cargo test -p <crate>` (scope to a
  module path with an extra filter, e.g. `cargo test -p brightflow-api --lib auth::password`).

### Frontend shape

- `/** ... */` header comment on each module under `brightflow-app/src/`.
- Tests are co-located as `*.test.ts` next to the module they cover and run via
  `npm run test` (the `vp test` built-in, Vitest under the hood). Config lives in
  the `test:` block of `vite.config.ts` — no separate `vitest.config.ts`.
- Import the Vitest primitives explicitly (`import { describe, test, expect } from 'vitest'`);
  globals are not injected.
- Use `environment: 'node'` for pure utilities; switch to a DOM env
  (`happy-dom`/`jsdom`) only when a component test lands.
- Tests are **unit only** — no Playwright or other E2E/browser runner, and no
  new test-runner dependency. A Pinia store is tested in isolation by calling
  `setActivePinia(createPinia())` in a `beforeEach` and reading its computeds
  (`brightflow-app/src/stores/query.test.ts` is the canonical example), never
  by mounting the app.

### Carve-outs (no unit test required)

- Axum handlers and other thin HTTP wiring — exercise via integration tests in
  `crates/*/tests/` instead.
- `mod.rs` re-export files and `types.rs` aggregations.
- Generated code under `brightflow-app/src/types/generated/`.
- Frontend `.vue` components (closer to integration tests; out of scope for the
  unit-test rule until a DOM test runner is wired).

## Workflow

- When debugging or testing changes, run both backend and frontend as background tasks to monitor output
- Vite 8 built-in `server.forwardConsole` forwards browser console output to the Vite terminal
- After Rust work is complete and debug compilation succeeds, always finish with `cargo build --release`

## Commands

```bash
# Rust
cargo check                       # verify compilation
cargo build --release             # release build (always run after debug succeeds)
cargo fmt --check                 # check formatting
cargo clippy                      # lint
cargo audit                       # check dependencies for vulnerabilities
cargo test -p <crate>             # run a single crate's unit tests

# Topics / intent classification
cargo run -- topics fit --source <s> --table issues        # refit + train the head
cargo run -- topics eval-classifier --source <s> --table issues  # head vs baseline macro-F1
cargo run -- topics near-dup --source <s> --table issues   # near-duplicate report

# Frontend (from brightflow-app/)
npm run dev                       # vp dev server
npm run check                     # type-check + lint + format (vp check)
npm run check:fix                 # auto-fix lint + format issues
npm run fmt                       # auto-fix formatting
npm run test                      # vp test (Vitest, built-in)

# Backend dev server
cargo run -- run-all              # API + WebSocket server (or just `cargo run`)

# Git hooks
./scripts/install-hooks.sh        # install pre-commit hooks
```

## Logs

- `logs/backend.log.<date>` — backend (Rust/Axum) logs, daily rotation (keeps last 7)
- `logs/frontend.log` — frontend (Vite dev server + forwarded browser console) logs, size-rotated at 5 MB (keeps last 4)


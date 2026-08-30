# Brightflow

## Project Overview

Analytics platform with a Rust backend (Axum + Polars) and Vue 3 frontend.

## Tech Stack

- **Backend**: Rust, Axum, Polars, SQLite (Litehouse)
- **Frontend**: Vite+ (Vite 8), Vue 3, TypeScript, Nuxt UI 4, Tailwind CSS 4, Pinia

## Project Structure

- `crates/brightflow-cli` - Binary (run-all, serve, insights, connect, store, topics, create-admin, migrate-events, compact)
- `crates/brightflow-core` - Workspace paths (`WorkspacePaths`), zero-dep
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

Three rules every change should follow, lifted from the engine-crate habits to
repo-wide expectations:

1. **Inline comments are the primary documentation, and they are maintained
   with the code.** Modules, non-trivial functions, and non-obvious decisions
   get a comment explaining *why*, not *what*. For Rust that means a `//!`
   module doc on every `src/` file; for the frontend a `/** ... */` header on
   each module. External prose docs are for architecture and onboarding, not for
   explaining individual functions.

   Presence is the easy half. When you change code, re-read the comments
   covering what you changed and make them true again — correct them, or delete
   them when the reason they recorded is gone. A comment you can only keep by
   watering it down is one to remove. Same scope rule as the tests below: the
   comments on what you touched, never a tree-wide audit.

   And never write a *why* you have not confirmed. A header asserting a
   rationale inferred from a skim is exactly the failure rule 3 describes — a
   wrong doc is still believed, and this one is harder to catch because it sits
   next to the code it misdescribes. `scripts/check-conventions.sh` can only
   check that a comment exists; nothing mechanical can check that it is true, so
   this rule is the only thing standing between a header and a confident lie.

   **Comment on your own file only.** State the *contract* your code offers, not
   an *observation* about someone else's. "Callers must not propagate this
   error" is yours and stays true; "`lib.rs` logs it and continues" is a fact
   about `lib.rs` that rots the moment that line changes — and rots invisibly,
   because the person changing `lib.rs` has no reason to open your file. That is
   the one drift this rule cannot catch, since re-verification is scoped to what
   you touched. If a fact belongs to another module, put it there or leave it
   out. Worst of all is asserting an *absence* ("nothing checks this today") —
   it is wrong the instant someone adds the check. Absences are findings, not
   documentation: raise them, don't compile them in.
2. **Unit tests per piece the agent touches.** When you add or change pure
   logic, add or update a co-located unit test covering the new behavior. Don't
   leave a touched function without coverage; don't add tests for code you
   didn't touch unless that's the explicit task.
3. **A prose file must be unable to go stale.** Three kinds qualify, and a file
   has to be one of them:
   - **normative** — philosophy and intent, what we mean to do
     (`docs/human_ai_interaction.md`, `docs/ux-principles.md`);
   - **dated** — a plan or report, true as of a date, never updated after
     (`plans/`, `reports/`);
   - **generated** — derived from source, regenerated not edited
     (`brightflow-app/src/types/generated/`).

   Prose that describes *current state by hand* is none of these. It belongs
   inline, next to the code that makes it true — where it is visible to whoever
   changes that code, and where fixing the code deletes the note. Hand-written
   state docs drift silently, and a wrong doc is worse than no doc because it is
   still believed.

   This is a **staleness test**, not a file-count rule: it decides every case
   mechanically. If you want a browsable version of something that lives in
   source (an API surface, a schema, a config reference), generate it — never
   hand-write it. `scripts/check-conventions.sh` enforces the mechanical part on
   staged files, diff-scoped so it can never fail on code you didn't touch.

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
- Two tiers, one runner (`vp test`, Vitest; config in the `test:` block of
  `vite.config.ts`, tiers split via Vitest `projects`):
  - **Unit** (default): stores via `setActivePinia(createPinia())`
    (`brightflow-app/src/stores/query.test.ts`), never mounting the app.
  - **Integration** (below the UI, not E2E): the `services/api` client + Pinia
    Colada (server-data store) against a real backend over HTTP on a fresh
    `testdata/workspaces/test` copy. `node` env; no browser/Playwright/DOM.

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
./scripts/audit.sh                # supply-chain gate: fresh advisory DB, non-zero on any
                                  # un-ignored advisory. Ignores (with reasons + recheck
                                  # triggers) live in .cargo/audit.toml. The pre-commit
                                  # hook's audit line is notify-only and --no-fetch, so
                                  # it never blocks and never proves clean — use this.
cargo test -p <crate>             # run a single crate's unit tests

# Topics / intent classification
cargo run -- topics fit --source <s> --table issues        # refit + train the head
cargo run -- topics eval-classifier --source <s> --table issues  # head vs baseline macro-F1
cargo run -- topics near-dup --source <s> --table issues   # near-duplicate report

# Frontend (from brightflow-app/)
npm run dev                       # vp dev server
npm run check                     # type-check + lint + format (vp check + vue-tsc for .vue)
npm run check:fix                 # auto-fix lint + format issues
npm run fmt                       # auto-fix formatting
npm run test                      # vp test (Vitest, built-in)

# Backend dev server
cargo run -- run-all              # API + WebSocket server (or just `cargo run`)

# Git hooks
./scripts/install-hooks.sh        # install pre-commit hooks
./scripts/check-conventions.sh    # module-doc + prose-file conventions on staged files
                                  # (diff-scoped; also runs from the pre-commit hook)

# Test workspace
./scripts/build-test-template.sh          # rebuild testdata/workspaces/test through the
                                          # real CLI + HTTP paths (needs the embedding
                                          # model; see the script header)
./scripts/build-test-template.sh --check  # report drift between the committed template
                                          # and what the script produces
./scripts/test-env.sh setup|reset|status  # the persistent interactive copy at
                                          # data/workspaces/test
```

## Logs

- `logs/backend.log.<date>` — backend (Rust/Axum) logs, daily rotation (keeps last 7)
- `logs/frontend.log` — frontend (Vite dev server + forwarded browser console) logs, size-rotated at 5 MB (keeps last 4)


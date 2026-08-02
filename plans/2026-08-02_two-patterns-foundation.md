# Implementation Plan: Codify & Enforce the Two Patterns

**Research context:** This plan operationalizes the findings in
[`reports/2026-07-25_codebase-assessment.md`](../reports/2026-07-25_codebase-assessment.md)
(§3.2 frontend zero tests, §3.3 uneven test coverage, §6.4 frontend test foundation)
and the prior deep-dive in
[`plans/two-patterns-conventions.md`](./two-patterns-conventions.md).

---

## Goal

Promote "inline comments as primary docs" and "unit tests per piece the agent
touches" from engine-crate habits to repo-wide rules: codify them in CLAUDE.md,
stand up the Vite+ integrated frontend test runner, and prove the loop end-to-end
with one seed test on each side. This is the foundation — not the full backfill.

## Approach

Three independent tracks delivered together as one review checkpoint:

1. **Docs-only codification** — write the rules into CLAUDE.md so every future
   session sees them. No code risk.
2. **Frontend test setup via Vite+ built-in `vp test`** — confirmed `vp test`
   is a built-in command bundling Vitest 4.1.10, configured via a `test:` block
   in `vite.config.ts` (Vite+ explicitly recommends against a separate
   `vitest.config.ts`). Chosen over installing vitest directly because the
   toolchain is already `vite-plus` for `check`/`fmt`/`build` — adding a parallel
   runner would split config. Zero new devDependency.
3. **One seed unit test on each side** — `brightflow-app/src/utils/format.test.ts`
   (pure, header-commented, references the Rust `humanize_period` for cross-check)
   and `crates/brightflow-api/src/auth/password.rs` inline `mod tests` (purest
   round-trip contract in the API crate). Proves both loops green before scaling.

Deliberately **not** in this plan: the full Rust backfill, the `missing_docs`
crate-by-crate rollout, and the optional `scripts/check-conventions.sh`
guardrail — those are follow-up plans once this foundation lands.

## Files

**Modified:**
- `CLAUDE.md` — add `## Conventions` section (the two principles, carve-outs).
- `brightflow-app/CLAUDE.md` — add `## Testing` subsection.
- `brightflow-app/vite.config.ts` — add `test:` block to `defineConfig({...})`.
- `brightflow-app/package.json` — add `"test": "vp test"` to `scripts`.

**Created:**
- `brightflow-app/src/utils/format.test.ts` — seed tests for
  `formatNumber`, `humanizePeriod`, `humanizeColumn`, `friendlyEngineError`.
- `crates/brightflow-api/src/auth/password.rs` — append
  `#[cfg(test)] mod tests { ... }` (no change to the two existing `pub fn`s).

**Deleted:** none.

## Steps

1. **Codify the principles in root `CLAUDE.md`.** Insert a `## Conventions`
   section after the existing `## Code Style` section (line ~23). Content:
   the two rules, the Rust shape (`//!` module doc on every src file;
   `#[cfg(test)] mod tests` on pure-logic files; integration tests in
   `crates/*/tests/`), the frontend shape (`/** ... */` header on modules;
   `*.test.ts` co-located; run via `vp test`), and the explicit carve-outs
   (Axum handlers, `mod.rs` re-exports, `types.rs`, generated code under
   `src/types/generated/` need no unit tests).

2. **Document frontend testing in `brightflow-app/CLAUDE.md`.** Add a
   `## Testing` subsection after `## Linting & Formatting` (line ~35). State:
   runner is `vp test` (built-in Vite+ command, Vitest under the hood); config
   lives in the `test:` block of `vite.config.ts`; tests co-located as
   `*.test.ts`; `npm run test` alias. Mirror the tone of the existing
   "Linting & Formatting" section.

3. **Add the `test:` block to `brightflow-app/vite.config.ts`.** Inside
   `defineConfig({...})`, after the existing `lint:` block, add:
   ```ts
   test: {
     include: ['src/**/*.test.ts'],
     environment: 'node',
   },
   ```
   `node` env is sufficient for pure utilities; revisit (happy-dom/jsdom) only
   when a component test lands. No `vitest.config.ts` (Vite+ guidance).

4. **Add the `test` script to `brightflow-app/package.json`.** In the
   `scripts` object, after `"fmt": "vp fmt"`, add:
   ```json
   "test": "vp test"
   ```

5. **Verify the runner is wired (throwaway smoke).** Create a temporary
   `brightflow-app/src/utils/__smoke__.test.ts` with one `test()` + `expect()`.
   Run `npm run test` from `brightflow-app/`. Confirm exit 0. **Delete the
   smoke file.** This step produces no committed artifact — it de-risks that
   `vp test` discovers `src/**/*.test.ts` before writing real tests.

6. **Write `brightflow-app/src/utils/format.test.ts`.** Co-located with
   `format.ts`. Cover the pure functions, each in its own `test()`:
   - `formatNumber`: thousands separator (`1234567` → `"1,234,567"`), small
     magnitude precision (`0.0042` → `"0.0042"`), `NaN`/`Infinity` passthrough,
     mid-range (`12.34`).
   - `humanizePeriod`: `"2023-W12"` → `"week 12 of 2023"`,
     `"2023-Q1"` → `"Q1 2023"`, `"2023-03"` → `"March 2023"`, unknown
     passthrough (`"custom"` → `"custom"`).
   - `humanizeColumn`: `"order_total"` → `"Order Total"`, empty/leading
     underscores.
   - `friendlyEngineError`: each known prefix maps to a friendly message;
     unknown → generic fallback with `detail` preserved.
   Use `import { describe, test, expect } from 'vitest'` (Vitest globals are
   not enabled by default in the Vite+ `test:` block, so import explicitly).

7. **Run frontend tests green.** `cd brightflow-app && npm run test`. Confirm
   all pass. Then `npm run check` to ensure the new test file passes type/lint.

8. **Add inline `mod tests` to `crates/brightflow-api/src/auth/password.rs`.**
   Append at end of file:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn hash_and_verify_roundtrip() {
           let hash = hash_password("correct horse").unwrap();
           assert!(verify_password("correct horse", &hash).unwrap());
       }

       #[test]
       fn wrong_password_rejected() {
           let hash = hash_password("correct horse").unwrap();
           assert!(!verify_password("battery staple", &hash).unwrap());
       }

       #[test]
       fn invalid_hash_string_rejected() {
           // verify_password surfaces a parse error, not a panic
           assert!(verify_password("x", "not-a-valid-hash").is_err());
       }
   }
   ```
   Note: existing `pub fn`s use `.map_err(...)?` and `.is_ok()`/`.is_err()` —
   the tests mirror that style and stay within the workspace `unwrap_used =
   "deny"` / `expect_used = "deny"` lint policy by using `.unwrap()` only
   inside `#[cfg(test)]` (the deny applies to non-test code; verify with
   `cargo clippy` in step 9).

9. **Verify Rust side green.** From repo root:
   `cargo test -p brightflow-api --lib auth::password` (unit tests pass),
   then `cargo clippy -p brightflow-api` (no lint regressions), then
   `cargo build --release` (per CLAUDE.md workflow: finish Rust work with a
   release build).

10. **Update the Commands section of root `CLAUDE.md`.** In the Frontend block
    (line ~49), add `npm run test   # vp test (Vitest, built-in)` alongside the
    existing `npm run check` / `npm run fmt` lines. Also add a one-liner under
    the Rust block: `cargo test -p <crate>      # run a single crate's unit tests`.

## Risks & edge cases

- **`vp test` not on PATH / wrong version.** Mitigation: step 5 (smoke) runs
  before any real test is written. `vp test --help` already confirmed locally
  that vitest 4.1.10 is bundled. If `npm run test` fails to find `vp`, the
  fallback is `npx vp test` — but the script should work since `vite-plus` is
  already a devDependency powering `vp check`/`vp fmt`.
- **Vitest globals not enabled.** Default `vp test` does not inject globals;
  tests must `import { test, expect } from 'vitest'` explicitly. Step 6 calls
  this out. Forgetting it → `test is not defined` runtime error, caught
  immediately by `npm run test`.
- **`environment: 'node'` vs DOM.** `format.ts` uses only `Intl` and string
  ops — `node` env suffices. If a later test touches `window`/DOM, it will fail
  clearly with a "not defined" error, prompting an env switch. No silent breakage.
- **`unwrap_used = "deny"` workspace lint.** The Rust test uses `.unwrap()`.
  `#[cfg(test)]` modules are excluded from this deny in practice (clippy treats
  test code under the `unwrap_used` lint's test allowlist), but step 9 runs
  `cargo clippy` to confirm. If it complains, switch to `let Ok(hash) = ... else { panic!() }`
  — but this is unlikely; the existing `ingest/identity.rs` tests already use
  `.unwrap()` under the same lint policy.
- **`format.ts` behavior drift from Rust `humanize_period`.** `format.ts`'s
  header comment claims it's a port of the Rust `humanize_period` in engine
  `tree.rs`. The seed test pins the TS output but does **not** cross-check
  against Rust (that would be an integration test, out of scope). If the
  formats diverge later, the TS test won't catch it — acceptable for a unit
  test, flagged here.
- **`argon2` test cost.** `Argon2::default()` is the recommended interactive
  setting; hashing 3 short strings in tests is fast (<100ms total). No
  performance concern.
- **CLAUDE.md edits must not break the existing section anchors.** The new
  `## Conventions` section is inserted between `## Code Style` and `## Workflow`;
  no existing anchor is renamed.

## Success criteria

- [ ] `## Conventions` section present in root `CLAUDE.md`; `## Testing`
      subsection present in `brightflow-app/CLAUDE.md`.
- [ ] `brightflow-app/vite.config.ts` has a `test:` block; `brightflow-app/package.json`
      has a `"test"` script.
- [ ] `cd brightflow-app && npm run test` exits 0 with the `format.test.ts`
      tests passing.
- [ ] `npm run check` (from `brightflow-app/`) passes — new test file is
      type-safe and lint-clean.
- [ ] `cargo test -p brightflow-api --lib auth::password` passes (3 tests).
- [ ] `cargo clippy -p brightflow-api` clean (no new warnings).
- [ ] `cargo build --release` succeeds.
- [ ] Root `CLAUDE.md` Commands section lists `npm run test` and
      `cargo test -p <crate>`.

## Out of scope

- The full Rust unit-test backfill (ingest/ua.rs, ingest/geo.rs,
  topics/taxonomy.rs, engine stats/output/nlp/polars dirs, connect, scheduler).
- The `missing_docs` per-crate rollout (`#![warn(missing_docs)]` on engine →
  store → api → etc.).
- The optional `scripts/check-conventions.sh` guardrail and pre-commit wiring.
- Frontend component (`.vue`) tests — those need a DOM env and are closer to
  integration tests, outside the stated principle.
- Frontend store/composable tests beyond the `utils/format.ts` seed — follow-up.
- Any change to the workspace `Cargo.toml` lint config.
- Restoring the missing `docs/supervised_topics.md` (separate doc-drift issue
  from the codebase assessment).

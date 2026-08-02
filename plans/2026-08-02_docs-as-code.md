# Documentation-as-code, sharpened

**Date:** 2026-08-02
**Status:** implemented

## The problem with the old rule

"No extra documentation files" is the right instinct but isn't enforceable, because it
doesn't separate three things that all look like docs. `crates/brightflow-api/API.md`
documented ~10 of 95 routes and still carried a "YC Dataset Columns" section from a
retired demo dataset — exactly the drift the rule exists to prevent, under a rule that
couldn't say why it was wrong.

## The rule

Replaced with a **staleness test** that decides every case mechanically:

> A prose file is allowed only if it cannot go stale: it is **normative** (philosophy —
> what we intend), **dated** (a plan or report — what was true on a date), or
> **generated** from source. Prose that describes current state by hand belongs inline,
> next to the code that makes it true.

Written into the `## Conventions` section of `CLAUDE.md` as rule 3, alongside the two
existing principles. `docs/readme.md` — which already half-stated it ("human written docs
that set the philosophy") — now states it as *the* rule and lists what's in the folder.

## Deletions and moves

- **Deleted `crates/brightflow-api/API.md`.** One referent (`brightflow-app/CLAUDE.md:67`).
  `routes.rs` is already a readable index of all 95 routes and `src/types/generated/` is
  already the typed contract. If a browsable API surface is wanted later, generate it
  (`utoipa`/`aide` off the axum router) — never hand-write it. `brightflow-app/CLAUDE.md`
  now points at `routes.rs` and says why there is no hand-written API doc.
- **`brightflow-app/CLAUDE.md:57`** cited `sources.ts`, which does not exist. It is
  `toolsForSource()` in `src/types/index.ts`.
- **`docs/subtext_research.md` → `reports/2026-04-15_subtext_research.md`.** A research
  snapshot; dating it makes it safe. A header note marks it unmaintained.
- **Kept** `docs/human_ai_interaction.md` and `docs/ux-principles.md` — normative.

## Inlined `docs/insights_limitations.md`, then deleted it

All nine entries had verified homes:

| Limitation | Home |
|---|---|
| Bonferroni is per-measure only | `analysis/drivers.rs` module doc |
| Null-model calibration synthetic-only | `analysis/null_models.rs` module doc |
| Seasonality assumes regular spacing | `analysis/seasonality.rs` (new module doc) |
| Drivers depth is one dimension | `analysis/candidates.rs` (`pair_aggregates`) |
| Null segment values excluded | `analysis/candidates.rs` (`DimensionIndex`) |
| Ambiguous slash dates parse day-first | `analysis/period.rs` (`parse_date_string`) |
| Auto-runs compute Trends only | `brightflow-api/src/insights/auto.rs` module doc |
| Measure polarity is display-only | `analysis/scoring.rs` (`kpi_boost_for`) |
| Legacy fingerprints activated retroactively | **dropped** |

Each inlined limitation keeps the original's three-part shape: what the limitation is,
why it exists, what would lift it. Preserving that honesty is the point — and a limitation
now disappears when someone fixes the code, because the note is right there.

The last entry was dropped: it is a dated migration note about a past change ("ranking
will shift as these findings start decaying"), not a standing limitation. By the staleness
rule it belongs in commit history, not permanent prose. That it falls out cleanly is a
good check on the rule.

`seasonality.rs` and `period.rs` had no `//!` header; both got one here, since the new
guardrail (correctly) refuses a staged `.rs` without one. That is the rule working on its
first commit rather than being grandfathered.

## `scripts/check-conventions.sh`, wired into `scripts/pre-commit`

**Diff-scoped only** — it inspects staged files via `git show :<path>` (the index, which
is what's being committed), never the whole tree. So it lands green today with the
109-file module-doc backlog still outstanding, and the backlog cannot grow while Plan 4
burns it down. A tree-wide version would fail on unrelated files, get bypassed, and
enforce nothing; there is a comment in the script saying so.

- Staged `.rs` under `crates/*/src/` → requires `//!` within the first 3 lines.
- Staged `.ts` under `brightflow-app/src/` (excluding `src/types/generated/`) → requires
  a `/** */` header.
- **New** `.md` (`--diff-filter=A`) outside `docs/`, `plans/`, `reports/` → rejected.
  `CLAUDE.md` / `README.md` at any level are allowed by name — tooling and humans look
  for those. Scoped to *new* files so this constrains what gets added rather than forcing
  a migration.

## Verification

- `cargo test --workspace` — the inlined comments are doc-only, but the engine's 286
  tests confirm no accidental code edits
- `bash scripts/pre-commit` green
- Staged a `.rs` with its `//!` stripped → hook rejects it (checked)
- No file outside `docs/`, `plans/`, `reports/` references `API.md` or
  `insights_limitations.md`; the remaining references are in the dated
  `reports/2026-07-25_codebase-assessment.md`, which is allowed to describe what was true
  on its date

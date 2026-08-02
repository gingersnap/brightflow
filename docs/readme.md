# Docs

This folder holds **normative** prose: human-written docs that set the philosophy and
intent of the project — what we mean to do, not what the code currently does.

That restriction is the rule, not a preference. A prose file is allowed in this repo only
if it cannot go stale:

- **normative** — philosophy and intent. Lives here.
- **dated** — true as of a date, never updated after. Lives in `plans/` or `reports/`.
- **generated** — derived from source, regenerated rather than edited. Lives next to what
  generates it (e.g. `brightflow-app/src/types/generated/`).

Anything else — prose that describes current behavior by hand — belongs **inline**, in a
`//!` module doc or a `/** ... */` header next to the code that makes it true. There it is
visible to whoever changes that code, and fixing the code deletes the note. A hand-written
state doc drifts silently, and a stale doc is worse than none because it is still believed.

If you want a browsable version of something that lives in source, generate it. Don't
hand-write it. See the `## Conventions` section of the root `CLAUDE.md` for the full rule
and `scripts/check-conventions.sh` for the mechanical check.

## Contents

- `human_ai_interaction.md` — how the product should behave when the machine has an
  opinion; normative.
- `ux-principles.md` — interface intent; normative.

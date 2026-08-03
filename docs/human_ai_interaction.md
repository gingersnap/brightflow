# Brightflow human–AI interaction principles

How humans and AI work together in Brightflow. The model is pair analysis, not
automated analysis: like a coding agent session, but for GUI data analysis. The
human and the AI co-edit the interpretation layer (labels, clusters, taxonomies,
annotations) on top of immutable data.

## Principles

1. **One action bus.** Humans and AI perform the same actions through the same
   dispatch path — everything goes through the bus by default; exceptions are
   explicit hard rules, not drift.
2. **Everything logged, one feed.** Every action (human, AI, background job)
   lands in a single activity stream with actor and provenance.
3. **Undo over approval.** Undoable actions run immediately (pre-approved by
   default) with one-step undo; approval gates are reserved for irreversible
   actions — or opted into per user/action as a trust dial.
4. **AI output is editable, not just acceptable.** Every AI result (labels,
   names, groupings) can be refined in place — accept, undo, *or* edit — and the
   edit is itself a logged action that overrides the AI's. Human corrections
   double as training signal (the supervised intent classifier already works
   this way).
5. **GUI first, keyboard rewarded.** Fully mouse-usable; command bar, shortcuts,
   and context menus make power users faster, never a prerequisite.
6. **Speed is the feature.** Sub-100ms feel; actions and agent activity push
   over WebSocket, never polled.

## The two layers

- **The engine (autonomous, read-only).** Cheap deterministic statistical
  methods run over everything and rank findings by signal. It never mutates
  state, so it needs no approval or undo — it only proposes findings into the
  feed. Rank by effect size with multiplicity control (Bonferroni-style
  correction for the number of comparisons), not raw p-values;
  dismissals and suppressions feed back into ranking.
- **The workspace (interactive, pair analysis).** Where state changes happen —
  through the bus, logged, undoable, editable.

The engine plays the role the deterministic toolchain (compiler, linter, tests)
plays for coding agents, in both directions: as *generator* it surfaces leads,
as *verifier* it grounds the AI's interpretive work (classifier evals, cluster
quality metrics). Stochastic proposes, deterministic grounds — the LLM narrates
and interprets, but is never the source of a statistical claim.

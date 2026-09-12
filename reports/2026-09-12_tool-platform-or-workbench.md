# Tool, Platform, or Workbench? What Brightflow Is, and What Follows for Licensing

**Date:** 2026-09-12
**Scope:** A conversation, written up. The starting question was whether
Brightflow is a data platform, an analytics tool, or something else, and what
the label changes. It grew into a description of the shape both of the author's
products share (Brightflow for data, Landline for team communication), the
layering of primitives beneath them, and a licensing and contribution model
that falls out of that layering. Positions here are the author's; this report
records them and the reasoning, so they can be checked against later decisions.
**Method:** Discussion between the author and Claude, grounded in the two
2026-09-08 reports and in a small number of code checks made during the
conversation (enrichment's materialisation path, the crate dependency graph,
the store crate's module layout, Cargo manifests). Each code claim below names
what was checked. No code changed.
**Baseline:** HEAD `6946e4d`. Builds on
`reports/2026-09-08_data-platform-completeness-and-operations-language.md`
(the layer-by-layer table and the gap ranking) and
`reports/2026-09-08_litehouse-as-semantic-layer.md` (the catalog-versus-
interpretation split inside Litehouse). Both are re-read here through a
different lens and one of their conclusions is reordered as a result.

---

## 1. Tool versus platform: the test that holds

Definitions by feature list do not separate the two, because a tool can have
every platform layer internally. The distinction that survives is about
consumers.

- A **tool** has one consumer, the user at the screen. Its value is the
  workflow it gives them.
- A **platform** has other things built on it by people who do not edit its
  code. Its value is the contracts it offers them: APIs, file layouts, schemas,
  stability promises. A platform is a substrate.

The test: name a second consumer that is not yours.

Applied to Brightflow, the completeness report's own table gives the answer.
Every layer is present, and every consumer is first-party: the Vue app, the
insights engine, the LLM agent. The serving row reads "covered for the app; no
outward door." There is no SQL endpoint, no export, no saved query, no second
application. By the consumer test Brightflow is a tool, and the platform-shaped
internals are internals.

A second, quieter sign points the same way. The semantic-layer report found
meaning defined in the API, the engine, and the frontend, not only in
Litehouse, and Polars expressions written inline in handlers. A platform layer
is one a second app could sit on without reading the first app's code.
Storage and catalog are that clean; semantics and transformations are not yet.

### Why the label matters

It decides what gets built. A platform invests in contracts and stability:
the folder layout as a documented promise, versioning and time travel, an
outward door, backward compatibility. A tool invests in the workflow: Explore,
the insights feed, enrichment quality, saved views. Calling a tool a platform
produces outward doors nobody walks through and stability guarantees that slow
the thing users touch. Calling a platform a tool produces breaking changes to
a folder other systems already read.

The primitives argument softens this: plain Parquet in a folder plus a SQLite
catalog is already readable by DuckDB and anything else, with no serving code.
The moment the folder layout and Litehouse schema are written down as a
contract, Brightflow is a platform in the honest sense. Until then they are an
implementation detail and "tool" is the truthful word.

## 2. "Tool" does not mean "one thing"

The author's objection: Brightflow does many things, and "tool" sounds like a
single-purpose utility. The resolution is that a tool's unity is a workflow,
not a feature count.

Sublime Text has hundreds of features and is one tool because every one of
them serves editing text fast. Basecamp is chat, to-dos, docs, schedules,
message boards, and check-ins, and 37signals call it one thing because every
piece serves running a project with a small team. iA Writer chose a narrow
workflow and defends it, but the narrowness is a choice, not what makes it a
tool.

So the test for adding something is whether it serves the workflow or starts a
new one. For Brightflow the sentence is roughly: *one person understands what
is happening in their business without a data team.* Explore, the insights
feed, enrichment, and the web-analytics dashboard all belong, as different
angles on the same act. A second user-role system or an outward SQL door would
not, because they serve someone else's workflow.

The failure mode of "I add a lot of stuff" is not accidentally becoming a
platform. It is becoming a **suite**: many things sharing a login but not a
workflow. 37signals' defence is that every feature must be an opinion about
how the work is done, not an option. Sublime's is that features should not add
UI. Brightflow will want its own version of that rule, because focus is what
decays when adding is enjoyable.

Extensibility is not a contradiction. A plugin API is a door into the tool,
not the tool becoming a substrate for other applications. Lua connectors and
versioned enrichment functions are that kind of door: tool features, not a
platform promise.

"One DB, one language" is the same instinct as 37signals': constraint lets you
have opinions, and an integrated tool can be optimised end to end because it is
generic nowhere. A platform must be generic at every boundary because it does
not know who is on the other side. The two cannot be had at once.

## 3. The workbench: many small tools, one substrate

The author's refinement: Brightflow is many small analysis tools (automatic
insights, text enrichment, more coming) that could each be broken out, held
together by one workflow. That shape has a name, and it is still a tool.

The shape is the **workbench**: Excel, Photoshop, an IDE, Sublime with its
packages. Excel is the closest analogy. Formulas, pivot tables, charts,
Solver, and Power Query are each a small tool that could ship alone. Excel is
one tool because every small tool reads and writes the same grid. The small
tools are not integrated with each other; each is integrated with the
substrate, and that is enough. Unix is the extreme case: tiny tools that know
nothing of each other, composed only because they agree on text streams.

Brightflow's grid is the workspace: tables in Parquet, the Litehouse catalog,
the semantic layer, and the feed as the shared place where tools report. This
is what "a data platform inside a tool" meant in section 1: a platform whose
only consumers are the author's own small tools. That is exactly what an
internal platform is for, and it is why the semantic layer matters more under
this framing than under the pure-tool one. The semantic layer is the contract
between the tools.

**Checked:** text enrichment does not keep results in a side table. Its
`materialize` path in `crates/brightflow-api/src/enrichment/runner.rs` reads
the source table, fills classification columns from the cache, and rewrites
the table's Parquet through `replace_table_data`. Explore and the insights
engine therefore see enriched fields without knowing enrichment exists. That
is the Excel property, already true for this pair of tools.

### The rule that falls out

A small tool belongs in Brightflow if it reads tables plus semantics from the
workspace and writes back columns, derived tables, or findings in the feed,
and nothing else. If a tool needs a private channel to another tool, either it
is not small or the substrate is missing a concept. The second case is the
useful signal: it says what to add to Litehouse, not to the tool. The
semantic-layer report's finding that roles and polarity are defined in three
places is, under this lens, two tools disagreeing about the medium, which is
the one failure a workbench cannot afford.

"Could be broken out" is a design discipline, not a distribution plan.
Sublime's separately shipped packages each had to solve settings, updates, and
discovery alone, and the best of them were later absorbed into the core. Keep
the tools small in interface and shipped as one binary, one login, one feed.

### Consequence for the gap ranking

The 2026-09-08 completeness report ranked its gaps through the platform lens.
Through the workbench lens the order changes:

| Rises | Falls |
|---|---|
| Saved views and artifacts (the person at the screen feels them) | Outward door (serves consumers the author has chosen not to have) |
| Data-quality findings surfaced in the feed | Governance, roles, PII flags |
| Derived tables a user can name and keep (the modelling layer, but as a user-facing tool) | Versioning and time travel |
| Semantic layer as the tools' shared contract | Dependency-aware orchestration (needed only once models exist) |

## 4. Two workbenches, one form

The author is building a second product on the same stack: Landline, a team
communication workbench (chat, notes, local data, instant search). Brightflow
and Landline share stack and shape but **not data**; each has its own
workspace. That removes the second-consumer question from section 1 entirely.
The family is Basecamp and HEY, not Google Workspace.

Landline's metaphor is the garden and the stream (Caulfield, 2015). Streams
are append-only: chat, notifications. The garden is pruned: wiki, notes. One
search covers both. As a substrate definition that is unusually clean: two
item kinds with different lifecycle rules, one index that does not care which
it is looking at. Chat, notes, and data are then two lifecycles and one index,
not three products.

Brightflow has the same split, apparently unplanned. Stream side: Parquet
files that are UUIDv7-named and never edited in place, connector syncs, the
ingest buffer, the insights feed, the action log. Garden side: Litehouse's
interpretation tables, column semantics, taxonomy vocabularies with
definitions and aliases, suppressions, enrichment function versions. Curated,
edited, pruned. One caveat: immutability is at the file level and tables are
rebuilt as whole new files on merge or enrichment, so it is "append-only files,
rebuilt tables" rather than a pure log.

Caulfield's actual point carries over: the garden is built *from* the stream by
someone deciding a passing thing deserves to be kept and connected. The
integrating verb in Landline is whatever moves an item from stream to garden.
If it is weak, Landline is a chat app beside a wiki; if strong, a workbench.
Brightflow's version is the action bus: an insight in the feed becomes a
suppression, a vocabulary entry, a corrected column role. Feed to semantic
layer is its stream-to-garden move.

Two workbenches, one metaphor. That is what a form looks like when it belongs
to the builder rather than the product. Sublime is a workbench for text,
Basecamp for a project, Obsidian for notes with a folder of markdown as its
grid. The author's sentence is "I build workbenches," which is clearer than
"I build tools" and says more about what to do next.

On the word: keep it as the idea, be careful with it as a name. If every
product is "Something Workbench" it turns generic within two products.
37signals let the shape show in the products and became known for it.

## 5. Primitives, semi-primitives, tools

The author's layering, with one distinction added:

| Layer | What | Examples | Change cost |
|---|---|---|---|
| Primitives | Chosen, not built. Leaned on for their strengths. | Rust, Tokio, Axum, SQLite, Polars, Parquet, Vue, Nuxt UI, ECharts | n/a |
| Semi-primitives | Built, few, structural. Set how the tools use the primitives. | Litehouse (substrate), Longbow (door in) | Touches every tool |
| Tools | What the user interacts with. Enter through the substrate. | Explore, insight engine, text enrichment, web analytics | Local |
| Workbench | The product. One workspace, one feed, one binary. | Brightflow | n/a |

The distinction worth keeping: not all semi-primitives are the same kind.
Litehouse is substrate, Longbow is a door in, and the insight engine and
enrichment are tools. Treating them as peers "around the workbench" hides that
Litehouse is load-bearing in a way the tools are not. A change to its schema
touches every tool, because it is the contract they speak through. It is also
where the primitives principle bites hardest: it should stay a thin, honest
use of SQLite and Parquet because everything else assumes it.

The same principle gives a test for every new piece: is it a thin adaptation
of a primitive to this workflow, or a reimplementation of something the
primitive already does? The modelling-language question in the 2026-09-08
report is this test. If Polars' expression algebra is the primitive, the
operations chain is the thin adaptation and SQL would be the reimplementation.

Keep the count honest. Two semi-primitives is the right number for a workbench
this size. A semi-primitive that exists in order to be a library, rather than
because a tool needed it, is the platform reflex in disguise. Extract a third
the way the first two were extracted: after a tool has made it necessary.

## 6. Licensing: Apache-2.0 libraries, AGPL workbench

The author's intent: release the semi-primitives (Litehouse, Longbow) as
Apache-2.0 libraries and the workbench (Brightflow) under AGPL. This is a
well-worn pattern (MongoDB drivers versus server, Sentry SDKs versus Sentry,
Plausible). It does two things beyond the obvious.

### Publishing turns an internal semi-primitive into a contract

Today Litehouse and Longbow change whenever a tool needs them to, which is the
whole advantage of an internal substrate. Once on crates.io every change is a
version bump and a migration for strangers, and the platform question returns
at library scale. A library is far cheaper than a platform (a dependency, not
a runtime) but it still has consumers the author does not control. Cost is
proportional to contract size, so the cheap libraries to publish are the ones
with the smallest, most stable surface.

### That pressure forces a boundary that is wanted anyway

The semantic-layer report counted seven catalog tables and twelve
interpretation tables in Litehouse. **Checked:** the store crate's own layout
agrees; `crates/brightflow-store/src/db/` holds `catalog.rs` beside
`actions.rs`, `agent.rs`, `curation.rs`, `enrichment.rs`, and `insights.rs`.

The catalog half (tables, files, column stats, partition pruning over Parquet)
is generic and stable: the part a stranger could use and a competitor could
take without harm, because it is commodity infrastructure. The interpretation
half is the workbench's opinion: column roles, vocabularies, insight state, the
action log with undo. That is what is being sold.

So an open-source Litehouse is the catalog half plus a clean way for a product
to add its own tables on top, and the interpretation tables move into the AGPL
product. **The licence line and the architecture line are the same line.**
Deciding the licence decides the crate split.

The rule: put in the Apache libraries only what you would be content to see in
a competitor's product. Catalog over Parquet and a Lua connector runtime, yes;
adoption helps more than exclusivity. The insight engine, enrichment, and the
semantic layer, no; they are the workbench.

### The dependency arrows already point the right way

Apache code may never import AGPL code, so every would-be library must be free
of product dependencies. **Checked** in the Cargo manifests at `6946e4d`:

| Crate | Brightflow dependencies |
|---|---|
| `brightflow-connect` (Longbow) | none |
| `brightflow-engine` | none |
| `brightflow-llm` | none |
| `brightflow-store` (Litehouse) | `brightflow-core` only (plus test-support) |
| `brightflow-core` | none |
| `brightflow-scheduler` | core, connect, engine, store |

Nothing that would be a library depends on anything that would be product.
Two notes from the same check: no crate declares a `license` field yet, and
the store's dependency on `brightflow-core` for `WorkspacePaths` would tie a
published Litehouse to Brightflow's folder convention. That seam wants a look
before release.

### Trademark

Apache 2.0 grants no trademark rights. The names Litehouse, Longbow, and
Brightflow stay the author's regardless of what the code licence allows.

## 7. Contributions: open source, not open contribution

The author's position: Brightflow the product takes no contributions, being
the integrated tool. Litehouse and Longbow could.

This has a precedent inside the stack. SQLite is open source and closed to
contributions ("open source, not open contribution") and has been for
twenty-five years. Sublime is the same at the editor level with an open
package ecosystem around it. Rails takes contributions; Basecamp does not.
The pattern: the product is where the opinion lives, and opinions do not
merge. The libraries are where contracts live, and contracts improve when many
people hit their edges.

It simplifies the legal side. With no outside contributions to Brightflow the
author holds all its copyright, so dual-licensing or a commercial licence
later stays available with no contributor agreement. For the Apache libraries
no agreement is needed either: Apache 2.0 section 5 licenses every submitted
contribution under the same terms. A Developer Certificate of Origin sign-off
is the lightweight extra if relicensing is ever wanted, and that is the one
decision to make before the first pull request rather than after.

### What contributions to each library would actually be

**Longbow: connectors, not runtime.** Connectors are Lua, so a contributor
needs no Rust, which is what makes a community possible. But every connector
in the tree is an upstream API to maintain against. Sublime kept packages out
of the editor's repository, with a registry pointing at other people's repos.
The line to decide: runtime plus a handful of reference connectors in Longbow,
community connectors somewhere the author does not own.

**Litehouse: contributions to a format.** A catalog over Parquet in SQLite is
useful to a stranger only if its schema can be trusted, and the most valuable
contributions would be readers in other languages, a DuckDB extension, a
Python package. Those are consumers, and they make the schema a spec.
DuckLake is a spec with a reference implementation for this reason. The repo's
prose conventions already handle it: migrations are the source of truth and a
spec is generated from them, never hand-written. Publishing Litehouse means
publishing a format, a heavier promise than publishing a crate.

### The closed product pushes feature pressure into the open libraries

Someone who wants Brightflow to do something and cannot send a pull request
there will send it to Litehouse or Longbow instead, and it will often be a
good patch that belongs in the product. "Add semantic tables to Litehouse" is
the obvious one. The defence is a written scope for each library:

- **Litehouse** is the catalog. Interpretation belongs to whatever consumes it.
- **Longbow** runs connectors and hands back frames. What happens to them is
  not its concern.

Those two sentences are the boundary from section 6 and double as the
contribution policy.

## 8. Bottom line

- Brightflow is an **analytics workbench**: one workspace, one feed, a growing
  set of small analysis tools that talk only through the data. Not a platform,
  because it has no second consumer and the author has chosen not to have one.
  Not a suite, as long as every tool enters through the substrate.
- The **substrate** (Litehouse plus the semantic layer plus the feed) is the
  contract between the tools, and the place where the semantic-layer report's
  duplication findings actually hurt.
- The **gap ranking** from 2026-09-08 reorders under this lens: saved views,
  quality findings in the feed, and user-named derived tables rise; outward
  doors, governance, and time travel fall.
- **Licensing** follows the layering: Apache-2.0 for the catalog half of
  Litehouse and for Longbow, AGPL for the workbench including Litehouse's
  interpretation tables. The crate split and the licence split are one
  decision. Dependency arrows already comply; `license` fields and the
  `WorkspacePaths` seam do not yet.
- **Contributions**: SQLite-style product, Rails-style libraries,
  registry-style connectors. Each open exactly as far as its kind allows.

### Open questions left with the author

1. What is the one-sentence workflow for Brightflow? The draft above is a
   guess; the author's own list of which recent additions felt bolted on
   would answer it better.
2. Which upcoming analysis tools cannot be expressed as "read tables and
   semantics, write columns or findings"? Those name what Litehouse is still
   missing.
3. For Landline, what is the stream-to-garden verb, and is it strong enough?
4. Where do community Longbow connectors live, and how many reference
   connectors stay in the tree?

## 9. References

- Mike Caulfield, "The Garden and the Stream: A Technopastoral" (2015):
  https://hapgood.us/2015/10/17/the-garden-and-the-stream-a-technopastoral/
- SQLite, "Open-Source, not Open-Contribution":
  https://www.sqlite.org/copyright.html
- Apache License 2.0, section 5 (Submission of Contributions):
  https://www.apache.org/licenses/LICENSE-2.0
- Developer Certificate of Origin: https://developercertificate.org/
- DuckLake specification: https://ducklake.select/docs/stable/specification/
- Prior reports: `reports/2026-09-08_data-platform-completeness-and-operations-language.md`,
  `reports/2026-09-08_litehouse-as-semantic-layer.md`,
  `reports/2026-08-03_philosophy-and-simplification.md`

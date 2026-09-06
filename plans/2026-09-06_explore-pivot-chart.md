# Plan: Explore — pivot fix, Excel-style pivot charts, sorted by size

**Status:** Proposal — decisions marked *confirmed* are settled with the author
on this date; *default* are my assumptions until reviewed.
**Date:** 2026-09-06
**Context:** Follows the author's first attempt to chart classified neofetch
issues (~2.3k rows, 10 categories, ~75 subcategories) in Explore after
[`2026-09-06_text-enrichment-two-tabs.md`](2026-09-06_text-enrichment-two-tabs.md)
landed. Supersedes an unwritten earlier draft that detected hierarchy from
the data; this one follows the field buckets, the way Excel does.

---

## Context

What the review found, in the order it surfaced:

- **The wide pivot is transposed.** Polars 0.48's `pivot` takes `on`, then
  `index`, then `values`. `analytics/executor.rs::apply_pivot` passes the
  row fields as `on` and the column field as `index`, so every pivot with
  both buckets filled comes back with rows and columns swapped. The author's
  screenshot (Rows = category, Columns = subcategory) showed subcategories
  on the axis and categories in the legend. No test covers the pivot path.
  The nested table the author likes (two row fields, no column field) goes
  through `groupBy`, which is correct, so the bug only shows on the wide
  layout.
- **The chart already maps buckets the Excel way, partly.** `ChartView.vue`
  picks the first row field as X and every numeric column as Y when a
  column field exists, and turns stacking on. What it lacks: any ordering
  by size (bars and series come in the backend's order, alphabetical for
  categories), a legend that copes with 75 entries, and colour that says
  which bar a segment belongs to. Its axis auto-select falls back to the
  first string column when the pivot result set is empty, which is how the
  screenshot ended with X = subcategory and a single Y.
- **Sorting is a query-level afterthought.** The Summarize panel's Sort row
  emits one `sort` operation on one column. Excel sorts per row field, by
  label or by value, within the parent group for nested fields, and the
  chart follows the table. `PivotTable.vue` groups rows client-side by the
  first index column and accumulates subtotals; it never reorders.
- **Two row fields plus stacking has no Excel output.** Excel draws two row
  fields as a grouped axis with one bar per leaf; stacking needs a legend
  field. The author wants the leaves as segments of the parent bar, which
  is the natural chart of the nested table. Excel produces nothing useful
  in that configuration, so filling it contradicts nothing.
- **Colours come from the theme** as CSS custom properties
  (`--color-data-1..12`, `assets/miami.css`), resolved by the browser to
  whatever format Tailwind 4 emits. Nothing parses colour strings today.

Binding repo rules: pure logic in a `.ts` module with co-located tests
(rule 2); `.vue` files and Axum handlers are carve-outs; module headers
state this file's contract (rule 1); this file is dated prose (rule 3). The
`dataviz` skill is loaded before any chart code is written, per its trigger.

## Decisions reached

- **Confirmed — fix the pivot first**, with a unit test that pins which
  values become rows and which become columns.
- **Confirmed — the chart maps buckets as Excel does.** Axis = row fields,
  legend = column field, values = the series. Stack settings none / stacked
  / 100 % and the orientation switch stay as they are.
- **Confirmed — order by size, not by name.** Bars by their total, series
  by their total, segments by their value, largest first, `other` last
  among equals.
- **Default — sort is a property of the row and column fields**, as in
  Excel: by label or by value, ascending or descending, set on the field
  chip in the bucket. Nested row fields sort within their parent. The
  default for a new field is *by value, descending* (the author's ask; Excel
  defaults to label order, which stays one click away). The pivot table and
  the chart share the ordering, so what you see in one matches the other.
- **Default — ordering is applied client-side** in one pure function over
  the result set. The backend returns the aggregate unordered; a result set
  small enough to pivot is small enough to sort in the browser, and it keeps
  the Rust side to the one-line fix.
- **Default — three stacking states, by bucket shape, no toggle:**
  1. *Stack none*: Excel. One bar per innermost row item; with two row
     fields, a grouped axis (§4).
  2. *Stack on, column field present*: Excel. One series per column value,
     stacked, legend shown.
  3. *Stack on, no column field, two or more row fields*: the innermost row
     field becomes the segments of the outer field's bar. Excel has no
     meaningful output here; this is the extension (§3).
- **Default — series by position in state 3.** The largest segment of every
  bar is series one, the next is series two, and so on. Ten series instead
  of 75, each data point carries its own segment name for hover and labels,
  and segments sort largest-first per bar, which series-per-value stacking
  cannot do.
- **Default — legend scrolls, never hides,** in state 2 (ECharts
  `legend.type: 'scroll'`), which is closer to Excel than dropping it. State
  3 has no series field and therefore no legend.
- **Default — colour follows the bar when it can.** In state 3 always: one
  palette hue per bar, segments as descending opacity of it. In state 2
  only when every series has values in exactly one bar (a parent/child pair
  built wide); otherwise palette colour per series, as Excel. This is a
  rendering rule, it changes no data and no structure. Opacity, not colour
  math: it needs no parsing of theme colours and survives a theme switch.
  `other` is the theme's muted grey at every level.
- **Default — horizontal by default in state 3**, since parent labels run
  long. The switch still overrides. Other states keep today's default.
- **Default — the value field may equal the column field** (count of
  `subcategory` with Columns = subcategory) only if Polars accepts it; §0's
  test decides, and if it does not, the executor returns a `BadRequest` that
  names the fix ("count another column, e.g. the id").

## §0 — Pivot fix (1 commit, Rust)

`crates/brightflow-api/src/analytics/executor.rs::apply_pivot`: pass
`columns` as `on`, `index` as `index`. One line. Doc comment states the
Polars argument order so the next reader does not "fix" it back.

Tests, in the file's `tests` module (the function is pure over a
`DataFrame`):

- A six-row frame with `category` ∈ {a, b}, `subcategory` ∈ {x, y, z} and
  an `id`; pivot index = category, columns = subcategory, values = id,
  agg = count. Assert the result has a `category` column with two rows and
  columns `x`, `y`, `z`. Today this asserts the opposite and fails.
- Two index columns (category, language) survive as two leading columns.
- The `values == columns` case: assert either the counts or the error,
  whichever Polars gives, and pin it.

`cargo test -p brightflow-api --lib analytics::executor`; then
`cargo build --release` per `CLAUDE.md`. Ships alone so the wide pivot is
right in the table view before anything else lands.

## §1 — Per-field sort, shared by table and chart (2 commits)

### 1a. Pure ordering

`brightflow-app/src/utils/pivotOrder.ts` (new, with `pivotOrder.test.ts`):

- `FieldSort = { by: 'label' | 'value'; descending: boolean }`.
- `orderRows(rows, indexIdx: number[], valueIdx: number[], sorts: FieldSort[]) → number[]`
  — row indices in display order. Sort by the first index column using its
  `FieldSort` (value = the sum of the value columns over the group), then
  within each group by the second, and so on. `other` ties to last on
  value sorts.
- `orderSeries(columns, rows, seriesIdx: number[], sort: FieldSort) → number[]`
  — series (numeric column) indices ordered by total or by name.
- Tests: two nested fields sort within parent; label vs value; asc vs
  desc; `other` last; null label sorts last under both; a single field;
  empty rows.

### 1b. Store, chip, consumers

- `types/index.ts`: `PivotField` gains `sort?: FieldSort`. `stores/pivot.ts`:
  row and column fields are created with `{ by: 'value', descending: true }`;
  `setFieldSort(id, sort)`; the existing `pivot.test.ts`-style store test
  (add one if absent, per `stores/query.test.ts`) covers the default and the
  setter.
- `BucketDropzone.vue`: a sort control on each row/column chip — a
  `USelectMenu` with the four combinations, `xs`, next to the aggregation
  select the values chip already has.
- `PivotTable.vue`: rows go through `orderRows` before grouping, so group
  headers and rows within a group follow the field sorts. Subtotals are
  unchanged.
- `ChartView.vue`: X data and every series' data follow `orderRows`; series
  follow `orderSeries` with the column field's sort. The Summarize panel's
  query-level Sort row is left as is for the table view; it does not apply
  to pivot results, and its header comment says so.

## §2 — Excel mapping, legend and colour (1 commit)

`components/charts/ChartView.vue`, plus a pure `components/charts/chartColor.ts`
(new, tested) holding the two rules below.

- Axis selection reads the buckets, not the column dtypes: X = first row
  field, Y = every value column, no fallback to "first string column" when
  a pivot is configured. The `All` link stays for hand-built tables.
- Legend: `type: 'scroll'`, bottom, in state 2.
- `seriesConfinedToOneBar(rows, seriesIdx) → boolean` and
  `barHueSeries(...)`: when true, each series takes its bar's hue at an
  opacity from 1.0 down to 0.35 by its rank within the bar; `other` and null
  series take the muted colour. When false, palette per series. Tests on a
  wide parent/child fixture (confined) and a state × category fixture (not
  confined).
- Tooltip: axis trigger, formatter drops zero segments and appends share of
  the bar.
- Module header rewritten as the three-state contract.

Carve-out on the `.vue`; `chartColor.test.ts` covers the rules.

## §3 — Inner-row stacking (1 commit)

`components/charts/stackedRows.ts` (new, tested): from long group-by rows
(outer index, inner index, value) and the §1 ordering, build series by
position:

- `stackedRowSeries(rows, outerIdx, innerIdx, valueIdx, order) → { bars: string[]; series: { rank, data: { value, name }[] }[] }`.
  `data[i]` is the rank-th largest segment of bar `i`, or null when the bar
  has fewer segments.
- Tests: three parents with 1, 2 and 4 children give four series with nulls
  where bars are short; segment names travel with their values; `other`
  last in each bar; the 100 % case is just `stackStrategy` and needs no
  change here.

`ChartView.vue`: state 3 uses these series with bar-hue colouring, no
legend, label formatter showing the segment name when it is ≥ 8 % of the
bar, tooltip as §2, `horizontal` defaulting on the first time the state is
entered for a result set.

## §4 — Grouped axis for stack none with two row fields (1 commit, optional)

Excel's reading of two row fields without stacking: one bar per inner item,
outer labels spanning their group. ECharts has no native two-level category
axis; render the inner labels on the axis and the outer labels as a second
category axis with `interval` set so each outer label appears once per
group, with a split line at group boundaries. Pure helper for the label
positions in `stackedRows.ts` (tested); the rest is option building in the
`.vue`. Last, and skipped without loss if it turns out to be more fiddly
than useful — nothing else depends on it.

## Execution order & sizing

| Step | Commits | Depends on | Note |
|---|---|---|---|
| §0 | 1 | — | Rust; wide pivot correct in every view |
| §1 | 2 | §0 | ordering shared by table and chart |
| §2 | 1 | §1 | Excel mapping, scroll legend, colour rule |
| §3 | 1 | §1 | the extension; only place series are by position |
| §4 | 1 | §1 | optional |

Each commit: `npm run check`, `npm run test`; §0 also `cargo fmt --check`,
`cargo clippy`, `cargo test -p brightflow-api`, `cargo build --release`.
`./scripts/check-conventions.sh` runs from the pre-commit hook.

## Verification

- **Rust unit** — the three pivot tests in `executor.rs`.
- **Frontend unit** — `pivotOrder.test.ts`, `chartColor.test.ts`,
  `stackedRows.test.ts`, the pivot store test.
- **Frontend check** — `npm run check`.
- **Manual, in Explore on the neofetch table with both servers running as
  background tasks:**
  1. Rows = category, Columns = subcategory, Values = count of id, table
     view: categories down, subcategories across (today it is the reverse).
     Chart, stacked: one bar per category, largest at the top when
     horizontal, segments coloured by bar hue, scrolling legend, hover
     names the subcategory and share. 100 % gives the mix view.
  2. Rows = category then subcategory, no column, Values = count of id,
     pivot view: headers ordered by subtotal descending, rows within by
     count descending; flipping category to label order re-sorts the
     table and the chart together. Chart, stacked: same bars as (1) but
     segments largest-first inside each bar, no legend.
  3. Rows = state, Columns = category, stacked: legend colours per series,
     since every series has values in both bars; sorted by total.
  4. Stack none with two row fields (§4 if built): grouped axis.
  5. Count of `subcategory` with Columns = subcategory: whichever §0 pinned,
     the table shows it or the error names the fix.

## Out of scope

- **Date bucketing** in query operations ("categories per month").
- **Several value fields** — the client sends only the first today; Excel's
  "Σ Values" field is its own change.
- **Linking the chart to the table's collapse state** (a collapsed category
  drawn as one plain bar). Noted as a natural follow-up.
- **A deep link from the Classification tab** that pre-fills the pivot.
- **Colour parsing** of theme values. Opacity is the shading mechanism.

## Open questions

1. Default sort for a new field: by value descending (as planned) or label
   ascending like Excel? (Default: value descending.)
2. Should the chip's sort control also appear on the values chip, sorting
   series in state 2 when there is no column field? There is nothing to
   sort there, so no. Confirming the reading.
3. Opacity floor 0.35 and label threshold 8 %: untested against the dark
   theme; checked in the manual pass and adjusted there.

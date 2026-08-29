# Brightflow App

Vue 3 web application for Brightflow analytics platform.

Current features (per-source tools, see `ToolId` in `src/types/index.ts`):

- **Dashboard / Funnels / Retention / Users** - web/product analytics views
- **Explore** - interactive data exploration via Rust+Polars backend
- **Insights** - computed insight feed with curation
- **Topics** - topic modeling, taxonomy, curation queue
- **Text Explorer** - term-based text search
- **Enrich** - LLM/topic-model enrichment functions

## Stack

- **Vite+ (Vite 8) + Vue 3 + TypeScript** (strict mode)
- **Nuxt UI 4** as pure Vue (not Nuxt framework) - see `vite.config.ts` and `main.ts` for setup
- **Tailwind CSS 4** (CSS-first config, no tailwind.config.js)
- **Pinia** for state management
- **vue-echarts** for charts

## TypeScript

Strict TypeScript is enabled with all strict flags plus additional checks:

- `noUncheckedIndexedAccess` - array/object access returns `T | undefined`
- `exactOptionalPropertyTypes` - distinguishes missing vs undefined
- `noUnusedLocals` / `noUnusedParameters` - errors on dead code

Shared types in `src/types/index.ts`. Generated types from Rust (via ts-rs) in `src/types/generated/`. Run `npm run check` to verify.

### .vue type-checking (temporary second checker)

- tsgolint does not type-check `.vue` script blocks (full Vue support is a
  stated non-goal for oxlint), so `npm run check` appends `vue-tsc --noEmit`
  as a whole-project pass over the SFCs, and the pre-commit hook runs the same
  step. Motivated by two refactor breakages that shipped as runtime errors
  (commits 4cd8bb1, 3646810) because no automated check covered `.vue`.
- `typescript` is pinned to 5.9 because vue-tsc needs the classic JS compiler
  API, which TypeScript 7 (the Go port) does not expose — a stable API for
  framework checkers is expected in TS 7.1. The pin exists for vue-tsc (and the
  editor's tsserver reads it); tsgolint embeds its own native compiler and does
  not read the npm `typescript` package.
- **Removal trigger:** when Vite+/tsgolint can type-check `.vue`, or vue-tsc
  runs on TypeScript ≥ 7.1, drop the `vue-tsc` step (package.json `check`,
  `scripts/pre-commit`), unpin `typescript`, and delete this section.

## Linting & Formatting

- **Vite+** unified toolchain: Oxlint (linter), Oxfmt (formatter), tsgolint (type checker)
- Formatter config: semicolons: always, trailing commas: all, single quotes, 100 char line width, Tailwind class sorting enabled
- All config in `vite.config.ts` under `fmt`, `lint`, and `staged` blocks
- Path aliases resolved from `tsconfig.json` via `resolve.tsconfigPaths` (single source of truth)

Run `npm run check` to verify all (types + lint + format). Run `npm run check:fix` to auto-fix. Run `npm run fmt` to format only.

## Testing

`vp test` (Vitest 4.x via Vite+), no extra runner/dep and no
`vitest.config.ts` — config lives in the `test:` block of `vite.config.ts`;
split tiers there with Vitest `projects`. Imports: Vitest primitives explicitly
(globals not injected). Environments: `node` for utilities and integration; DOM
only for component tests. No Vitest `browser` mode — that's the E2E path we
avoid.

Two tiers, one runner:

- **Unit (default):** co-located `*.test.ts` next to the module; stores via
  `setActivePinia(createPinia())` (`src/stores/query.test.ts`), never mounting
  the app.
- **Integration (below the UI, not E2E):** the data layer — the `services/api`
  client + Pinia Colada (the server-data Pinia store) queries/mutations —
  against a real backend over HTTP: real `build_app` on an ephemeral port, a
  fresh `testdata/workspaces/test` copy, real session + storage round-trip.
  Asserted on data; no browser/Playwright/DOM/mounted components.

Integration plumbing: runtime API-base override (default `VITE_API_BASE`);
server booted in `globalSetup` on a throwaway workspace + ephemeral port, torn
down after; minimal server (no scheduler/WS unless targeted); session via
replayed `Set-Cookie` (Node `fetch` doesn't persist cookies); prove one
read+write+session round-trip before breadth.

## Architecture

**Multi-source, per-table routing** - the app is organized around sources, each
exposing one or more tables. Routes are `/:sourceId/<tool>/:table`
(`toolsForSource()` in `src/types/index.ts` maps a source to its tools). The
connection and query state are scoped to the active source/table pair, not a
single global dataset.

**WebSocket for queries** - REST felt too slow for interactive exploration. Connection managed in `stores/connection.ts`, query execution in `composables/useWsQuery.ts`.

**Server data is Pinia Colada's job** - all server data is fetched and cached by
Pinia Colada (`useQuery`/`useMutation`) in components and composables. Pinia
stores hold client state only (selections, overlays, UI flags, WS-event merge
state). Stores never import `services/api`. One paradigm, no per-case
decisions: if it came over the network, it lives in the colada cache; if the
user chose or toggled it, it lives in a store.

**Query builder as primary UX** - users build queries visually rather than writing code. Each section (filter, group by, sort, limit) is toggleable. See `stores/query.ts` for state shape and `components/query/` for UI.

## Key Files

- `../crates/brightflow-api/src/routes.rs` - The API surface. Every route in one readable
  index; there is no hand-written API doc, deliberately (it drifted).
- `src/types/index.ts` - Shared TypeScript types (frontend-only + re-exports from generated)
- `src/types/generated/` - TypeScript types auto-generated from Rust via ts-rs
- `stores/query.ts` - Query state and operations builder
- `composables/useOperators.ts` - Filter operators by column type
- `services/websocket.ts` - WebSocket client with reconnection

## Console Forwarding

Vite 8's built-in `server.forwardConsole` forwards browser `console.*` calls to the dev terminal. Configured in `vite.config.ts`. All frontend logs visible in the terminal without browser DevTools. Dev-only, no production impact.

## Nuxt UI Notes

Components use Nuxt UI 4 conventions:

- `UTable` uses TanStack Table format (`data` + `columns` with `accessorKey`)
- `USelectMenu` uses `items` prop (not `options`)
- Colors configured in vite plugin, not CSS variables

## Typography

Inspired by Vercel Geist and Linear — 12px is chrome-only, 14px is the default.

**Font-size utilities:**

- `text-sm` (14px) is the default for UI chrome: labels, captions, empty states, table headers, breadcrumbs, metadata under names, form help text.
- `text-base` (16px) for prose and primary readable content (dashboard card descriptions, longer explanations).
- `text-xs` (12px) is allowed only for:
  - Status chips and badges (`SyncStatusBadge`, Ready/Pending pills, interval chips)
  - Uppercase eyebrow section headers with `tracking-wider uppercase`
  - Kbd hints and key-combo indicators
  - Dense data grids where density is intentional (retention heatmap cells, log viewers)
  - Chrome-adjacent numerals next to an icon or inside parens (child-counters, `(N active)`, `Xms`)
- Do not use `text-xs` for form labels, empty-state prose, table data cells the user reads, button labels, or anything that's a full sentence.

**UButton sizes:**

Nuxt UI's UButton renders `size="xs"` and `size="sm"` at 12px text (only padding differs — `sm` is strictly worse than `xs`). Default `md` renders 14px.

- **Always** specify `size` explicitly on UButton — don't rely on the default.
- Default to `size="md"`.
- `size="xs"` only for buttons placed inside an input field (Vercel Geist's "Button 12" criterion).
- `size="sm"` is banned (same 12px text as `xs`, fatter padding).
- `size="lg"` / `size="xl"` for primary CTAs where extra weight helps.

**UBadge sizes:**

UBadge's default `size="md"` renders 12px text.

- Default to `size="lg"` (14px) for badges the user reads.
- `size="md"` only for status chips and decorative markers.
- `size="sm"` / `size="xs"` are banned for the same reason as UButton's `sm`:
  same 12px text as `md` with worse proportions.

**UTooltip:** hardcoded at 12px in Nuxt UI's theme. Accept as-is — tooltips are conventionally compact and transient.

## Nuxt UI First

Before hand-rolling any UI element (button, badge, tabs, table, form field, select,
collapsible, spinner), reach for the Nuxt UI 4 component and accept its default styling —
no `:ui` overrides to pixel-match old markup. Collapsible sections use
`src/components/common/CollapsibleSection.vue`. Loading states: `UButton :loading` when the
spinner belongs to an action, otherwise `UIcon name="i-lucide-loader-circle"` + `animate-spin`.
Raw markup is the exception and needs a justifying comment (e.g. PivotTable's domain grid).

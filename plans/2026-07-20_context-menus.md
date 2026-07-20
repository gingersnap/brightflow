# Context Menu Implementation

Implement right-click context menus across Brightflow's frontend using Nuxt UI 4's
`UContextMenu`. Two tracks:

- **(a) Mirror existing kebab menus** on cards that already have `UDropdownMenu` action
  buttons — `ClusterCard`, `InsightCard`. Items arrays already exist; near-zero new logic.
- **(b) Add context menus to action-less list/table rows** where they unlock new
  capability — `ResultRow`, `RunHistoryTable`, `DataTable`.

Plus keyboard-shortcut integration via `extractShortcuts`, so the same `items` arrays
double as the source of truth for `defineShortcuts`.

## Nuxt UI reference (confirmed)

- `UContextMenu` wraps content in its default slot; right-click anywhere in the slot
  opens the menu at the cursor. No trigger button needed.
- `items` prop shape is **identical** to `UDropdownMenu` — `label`, `icon`, `onSelect`,
  `children`, `type: 'separator' | 'checkbox' | 'label'`, `color`, `kbds`, `disabled`,
  `slot`. Existing `menuItems` / `curationItems` arrays are reusable as-is.
- `extractShortcuts(items)` (composable) recursively pulls `kbds` off items and returns
  an object compatible with `defineShortcuts(...)`. This makes a single items array the
  source of truth for both the context menu *and* keyboard shortcuts.
- **`UTable` row context menu** is a documented first-class pattern: add `@contextmenu`
  on the table, handler receives `(Event, TableRow)`, and wrap the `UTable` in
  `UContextMenu` controlled via `v-model:open`. This avoids one `UContextMenu` per row.
- Component is auto-registered via `app.use(ui)` in `src/main.ts` — no setup change.
- Keep `modal` default (`true`); Reka UI dismisses on outside-click correctly. Revisit
  only if a focus-trap issue surfaces inside `UModal`-hosted content.

## Conventions to follow (from `brightflow-app/CLAUDE.md`)

- Nuxt UI first — no hand-rolled menu markup.
- `UButton` always has explicit `size`; default `md`. `size="sm"` banned.
- `text-sm` (14px) default for menu labels; `text-xs` only for status-style items.
- Icons: `i-lucide-*` (the project ships `@iconify-json/lucide`).
- Replace `window.prompt` calls opened from a context menu with `TextPromptModal` /
  `ConfirmModal` (via `useOverlay`) — both already exist in `components/command/`.
  This is scoped to the items we touch, not a codebase-wide refactor.
- Strict TypeScript: items arrays typed as `ContextMenuItem[][]` (import the type from
  `@nuxt/ui`). Watch `noUncheckedIndexedAccess` when indexing captured row data.

---

## Phase 0 — Shared infrastructure

### 0.1 `useOverlay` wiring check
`TextPromptModal` / `ConfirmModal` are designed to be opened via `useOverlay()` (from
`@nuxt/ui`). Confirm this is the pattern already used elsewhere; if not, adopt it here
as the canonical prompt-replacement helper. These modals `emit('close', value)` where
the overlay promise resolves — see their doc comments.

### 0.2 New composable: `src/composables/usePromptAction.ts` (proposed)
A thin helper that wraps `useOverlay` + `TextPromptModal` so item handlers read cleanly:

```ts
// Returns (title, opts) => Promise<string | null>
export function usePromptAction(): (title: string, opts?: {
  description?: string; placeholder?: string; initialValue?: string;
}) => Promise<string | null> { ... }
```

This lets a context-menu item be:
```ts
{ label: 'Rename…', icon: 'i-lucide-pencil', kbds: ['meta', 'R'],
  onSelect: async () => { const name = await prompt('Rename cluster', { initialValue: cluster.name }); if (name) await renameCluster(name); } }
```

Used to migrate the `window.prompt` sites we touch in Phase 1.

---

## Phase 1 — Track (a): mirror existing kebab menus

For each card, **keep the kebab `UButton`** (touch-friendly, discoverable) and add a
`UContextMenu` wrapping the whole card root that reuses the same items array. The
items array is hoisted out of inline `const` into a `computed` so it stays reactive
and is importable for `extractShortcuts` in Phase 3.

### 1.1 `src/components/topics/ClusterCard.vue`
- Hoist `menuItems` → `const menuItems = computed(() => [...])` (already partially so
  in `InsightCard`; `ClusterCard` currently uses a plain `const`).
- Wrap the outer `<div class="overflow-hidden rounded-lg border ...">` in
  `<UContextMenu :items="menuItems">`.
- Migrate `renameCluster`, `assignLabel`, `mergeInto`, `excludeTerm` off
  `window.prompt` → `usePromptAction()`. `mergeInto` needs numeric validation; keep a
  small guard (already has `Number.isNaN` check).
- Add `kbds` to the high-value items (Rename ⌘R, Mark as noise ⌘E — see Phase 3 for
  final assignments to avoid collisions).

### 1.2 `src/components/insights/InsightCard.vue`
- `curationItems` is already a `computed`. Wrap the card root in
  `<UContextMenu :items="curationItems">`.
- Migrate `annotate()` and `suppressDimension()` off `window.prompt` →
  `usePromptAction()`.

### 1.3 (Optional, defer) Other kebab sites
`ColumnListPanel.vue`, `PromptEditor.vue`, `FunctionEditor.vue` already have
`UDropdownMenu` for *insert/add* flows rather than row-level object actions. Their
kebab-equivalent is a primary `+ Add` button, not a per-row menu. **Defer** — these
don't benefit from right-click the way object cards do. Revisit if row-level actions
are added later.

---

## Phase 2 — Track (b): context menus on action-less rows

### 2.1 `src/components/textexplore/ResultRow.vue` / `ResultsList.vue`
Currently zero actions. Right-click on a result row → clipboard + navigation:

- Copy title, Copy snippet, Copy link (only when `row.htmlUrl`), Open link in new tab.
- (Stretch) "Send to curation" — wire to `useCurationStore` if a doc-ref mapping
  exists; otherwise defer until the curation API accepts a raw `TextExploreRow`.

**Implementation**: wrap each `<ResultRow>` (or its `<article>`) in `UContextMenu`
with a per-row items `computed`. Row count is small (paginated "Load more"), so
per-row instances are fine — no shared-instance complexity needed here.

Items live in `ResultsList.vue` and are passed down, or `ResultRow` builds its own from
its `row` prop. Prefer the latter (locality) unless `ResultsList` needs to coordinate.

### 2.2 `src/components/connect/RunHistoryTable.vue`
Wrap the `UTable` in a single `UContextMenu` controlled by `v-model:open`, using the
documented `@contextmenu` row handler:

```vue
<UContextMenu v-model:open="open" :items="items">
  <UTable :data="runs" :columns="columns"
          @contextmenu="onContextMenu" />
</UContextMenu>
```

```ts
const open = ref(false);
const target = ref<EnrichedSyncRun | null>(null);
function onContextMenu(e: Event, row: TableRow<EnrichedSyncRun>): void {
  target.value = row.original;
  e.preventDefault();
  open.value = true;   // Reka UI positions at cursor automatically
}
const items = computed<ContextMenuItem[][]>(() => [
  [{ label: 'Re-run sync', icon: 'i-lucide-refresh-cw', onSelect: () => emit('run', target.value!.connectorName) }],
  [{ label: 'Copy error', icon: 'i-lucide-copy', disabled: !target.value?.error,
     onSelect: () => navigator.clipboard.writeText(target.value!.error!) }],
]);
```

Add `emit('run', name)` (or reuse an existing one). Small row counts → shared instance
is the right call here and demonstrates the pattern for future tables.

### 2.3 `src/components/results/DataTable.vue`
Two context menus:

- **Header cell**: "Sort ascending", "Sort descending", "Hide column", "Group by",
  "Copy column name". Uses TanStack `column.getToggleSortingHandler()` /
  `column.getCanHide()` etc. via the `#<column>-header` slot or a header `cell` render.
- **Row**: "Copy cell value", "Filter by this value" (writes into `useQueryStore`
  filter). Stretch — depends on store shape; keep "Copy cell value" for v1.

This is the most involved change. Recommend doing it **last** and possibly as its own
follow-up, since `DataTable` currently has no actions at all and wiring sort/group
into the query store is non-trivial. Land 2.1 and 2.2 first.

---

## Phase 3 — Keyboard shortcuts via `extractShortcuts`

### 3.1 Add `kbds` to items
Attach `kbds` to the high-value items added/touched in Phases 1–2. Proposed map
(subject to review, avoid collisions with existing `CommandPalette` shortcuts —
audit `src/components/command/paletteActions.ts` first):

| Action | kbds | Site |
|---|---|---|
| Rename cluster | `['meta', 'R']` | ClusterCard |
| Mark cluster as noise | `['meta', 'E']` | ClusterCard |
| Pin insight | `['meta', 'P']` | InsightCard |
| Re-run sync | `['meta', 'Enter']` | RunHistoryTable |

Only assign shortcuts where the action is unambiguous on the **currently hovered/
focused** element. `extractShortcuts` + `defineShortcuts` fire globally — so a
shortcut like ⌘R must be scoped. Use `defineShortcuts`'s `usingMeta`/`happenedWhile`
or guard inside `onSelect` by checking a hovered-element ref. **This is the main
design risk** — see Open Questions.

### 3.2 Wire `extractShortcuts`
In each component that owns an items array:

```ts
import { extractShortcuts, defineShortcuts } from '#imports'; // or '@nuxt/ui'
defineShortcuts(extractShortcuts(items.value));
```

Confirm the import path for non-Nuxt (pure Vue plugin) usage — `#imports` may not
resolve; the composable is exported from `@nuxt/ui` runtime. Verify against how
`defineShortcuts` is currently used (search the codebase first).

---

## Phase 4 — Verification

- `npm run check` (types + lint + format) — strict TS, oxlint pedantic.
- Manual: right-click each surface; confirm menu opens at cursor, items fire, outside-
  click dismisses, focus trap behaves inside `UModal` parents.
- Touch: long-press (`pressOpenDelay` default 700ms) opens on mobile — verify it
  doesn't fire during scroll on `ResultRow` list and `DataTable`.
- Keyboard: with `kbds` wired, confirm shortcuts fire only when the owning element is
  hovered/focused (no global misfires).

---

## Open questions / decisions needed

1. **Shortcut scope.** `defineShortcuts` fires globally by default. For ⌘R to mean
   "rename the *hovered* cluster" we need a hover/focus ref guard. Acceptable, or
   restrict shortcuts to a narrower set (e.g. only inside an open menu)? Affects
   Phase 3 effort materially.
2. **DataTable header actions scope.** Wiring "Group by" / "Sort" into the query store
   (`stores/query.ts`) may be out of scope for this plan. Confirm whether v1 of
   `DataTable` context menu is **clipboard + navigation only**, deferring query
   manipulation.
3. **ResultRow "Send to curation."** Does the curation API accept a `TextExploreRow`
   / `DocRef` mapping today? If not, defer that item (keep Copy/Open only).
4. **`window.prompt` migration scope.** Migrate only the items we touch
   (`ClusterCard`, `InsightCard`), or also the untouched prompt sites
   (`TaxonomyPanel`, `InsightCard.suppressDimension`)? Default: touched-only.
5. **Composable home.** `usePromptAction` in `composables/` — confirm naming/placement
   convention matches existing composables (`useWsQuery`, `useOperators`).

## Out of scope

- Replacing the kebab buttons themselves (they stay for touch + discoverability).
- `ColumnListPanel` / `PromptEditor` / `FunctionEditor` add-button flows (Phase 1.3).
- PivotTable cell drill-down (separate, larger effort — its grid is hand-rolled).
- Codebase-wide `window.prompt` → modal migration.

/**
 * Wires the command palette: builds the item list from the current route and
 * context, and dispatches the chosen action.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { type Ref, computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import ConfirmModal from '@/components/command/ConfirmModal.vue';
import {
  ACTION_PALETTE,
  type ConfirmOptions,
  INSIGHT_KINDS,
  type PaletteActionContext,
  type PaletteItem,
  type PromptOptions,
  VOCABULARY_KINDS,
} from '@/components/command/paletteActions';
import TextPromptModal from '@/components/command/TextPromptModal.vue';
import { patchFromAction, useInsightActions } from '@/composables/useInsightActions';
import { useSources } from '@/composables/useSources';
import { actionsApi, taxonomyApi } from '@/services/api';
import { useDatasetStore } from '@/stores/dataset';
import { useInsightsStore } from '@/stores/insights';
import { toolsForSource } from '@/types';
import type { Action } from '@/types/generated';

export interface PaletteGroup {
  id: string;
  label: string;
  items: PaletteItem[];
}

/**
 * Item assembly for the global command palette: navigation (always) plus a
 * context-scoped Actions group fed by the backend action manifest.
 *
 * Scope convention (same as the agent runner and InsightCard.actionScope):
 * `sourceId`/`table` come from the route — context-injected, never typed.
 * The insights store is a fallback only on insights views; route params win.
 * Without a full scope the Actions group is hidden, not disabled.
 */
export function useCommandPalette(
  open: Ref<boolean>,
  closePalette: () => void,
): { groups: Ref<PaletteGroup[]> } {
  const route = useRoute();
  const router = useRouter();
  const { sources } = useSources();
  const insightsStore = useInsightsStore();
  const datasetStore = useDatasetStore();
  const insightActions = useInsightActions();
  const queryCache = useQueryCache();
  const overlay = useOverlay();

  const promptModal = overlay.create(TextPromptModal);
  const confirmModal = overlay.create(ConfirmModal);

  // ── Route context ─────────────────────────────────────────────────────────

  const routeTool = computed<string | null>(() => {
    const name = typeof route.name === 'string' ? route.name : '';
    const tableRoute = /^(?<tool>insights|explore|textenrichment|textexplore)-table$/u.exec(name);
    if (tableRoute) {
      return tableRoute.groups?.['tool'] ?? null;
    }
    if (name === 'source-tool') {
      const tool = String(route.params['tool'] ?? '');
      return tool === '' ? null : tool;
    }
    return null;
  });

  const scopeSourceId = computed<string | null>(() => {
    const fromRoute = String(route.params['sourceId'] ?? '');
    if (fromRoute !== '') {
      return fromRoute;
    }
    return routeTool.value === 'insights' ? insightsStore.selectedSourceId : null;
  });

  const scopeTable = computed<string | null>(() => {
    const fromRoute = String(route.params['table'] ?? '');
    if (fromRoute !== '') {
      return fromRoute;
    }
    return routeTool.value === 'insights' ? insightsStore.selectedTable : null;
  });

  const hasScope = computed(() => scopeSourceId.value != null && scopeTable.value != null);

  // ── Data (cache keys identical to the owning views — warm cache = free) ───

  const { data: manifest } = useQuery({
    key: ['actions-manifest'],
    query: () => actionsApi.manifest(),
    enabled: () => open.value,
  });

  const isTextEnrichment = computed(() => routeTool.value === 'textenrichment' && hasScope.value);
  const isInsights = computed(() => routeTool.value === 'insights' && hasScope.value);

  const { data: taxonomy } = useQuery({
    // Same key as VocabularyTree.vue.
    key: () => ['taxonomy', scopeSourceId.value ?? '', scopeTable.value ?? ''],
    query: () => taxonomyApi.overview(scopeSourceId.value ?? '', scopeTable.value ?? ''),
    enabled: () => open.value && isTextEnrichment.value,
  });

  // ── Helpers handed to the action flows ────────────────────────────────────

  async function promptText(options: PromptOptions): Promise<string | null> {
    const result: unknown = await promptModal.open(options).result;
    return typeof result === 'string' ? result : null;
  }

  async function confirmAction(options: ConfirmOptions): Promise<boolean> {
    const result: unknown = await confirmModal.open(options).result;
    return result === true;
  }

  async function dispatchAction(action: Action): Promise<void> {
    const entry = (manifest.value ?? []).find((m) => m.kind === action.kind);
    // Insight-scoped actions get the optimistic overlay patch + Undo toast;
    // Everything else goes through the same path with feedback only.
    const patch = patchFromAction(action);
    const applied = await insightActions.dispatchWithFeedback(action, {
      title: entry?.label ?? action.kind.replaceAll('_', ' '),
      ...(patch == null ? {} : { patch }),
    });
    if (!applied) {
      return;
    }
    // Vocabulary views read through pinia-colada; insights views react to
    // The overlay + the WS event stream.
    await queryCache.invalidateQueries({
      key: ['taxonomy', action.source_id, action.table],
    });
  }

  // ── Groups ────────────────────────────────────────────────────────────────

  const navigationItems = computed<PaletteItem[]>(() => {
    const go = (name: string) => () => {
      closePalette();
      void router.push({ name });
    };
    const items: PaletteItem[] = [];
    for (const source of sources.value) {
      for (const tool of toolsForSource(source)) {
        items.push({
          label: tool.label,
          // The source name doubles as a fuse key: "text enrichment gh" finds it.
          suffix: source.name,
          icon: tool.icon,
          onSelect: () => {
            closePalette();
            void router.push({
              name: 'source-tool',
              params: { sourceId: source.id, tool: tool.id },
            });
          },
        });
      }
    }
    items.push(
      { label: 'Sources', icon: 'i-lucide-layers', onSelect: go('sources') },
      { label: 'Add source', icon: 'i-lucide-plus', onSelect: go('sources-new') },
      { label: 'Schedules', icon: 'i-lucide-calendar-clock', onSelect: go('schedules') },
      { label: 'System', icon: 'i-lucide-activity', onSelect: go('system') },
      { label: 'Activity', icon: 'i-lucide-history', onSelect: go('activity') },
      { label: 'Preferences', icon: 'i-lucide-settings-2', onSelect: go('preferences') },
    );
    return items;
  });

  const actionItems = computed<PaletteItem[]>(() => {
    const sourceId = scopeSourceId.value;
    const table = scopeTable.value;
    if (sourceId == null || table == null || manifest.value == null) {
      return [];
    }
    let kinds: readonly string[] = [];
    if (isTextEnrichment.value) {
      kinds = VOCABULARY_KINDS;
    } else if (isInsights.value) {
      kinds = INSIGHT_KINDS;
    }
    if (kinds.length === 0) {
      return [];
    }

    const ctx: PaletteActionContext = {
      sourceId,
      table,
      data: {
        // Insights come from the already-loaded store tree (no fetch);
        // Unloaded views simply offer no insight-targeted items.
        categories: taxonomy.value?.categories ?? [],
        insights: insightsStore.visibleRoots,
        columns: datasetStore.columns,
      },
      helpers: {
        dispatch: dispatchAction,
        promptText,
        confirm: confirmAction,
        close: closePalette,
      },
    };

    // The manifest drives iteration: the backend decides what exists and what it is called.
    // Kinds without a local flow config are skipped.
    return manifest.value.flatMap((entry) => {
      const config = kinds.includes(entry.kind) ? ACTION_PALETTE[entry.kind] : undefined;
      if (config == null) {
        return [];
      }
      const body = config.build(ctx);
      // Entity pickers with nothing to pick disappear rather than dead-end.
      if (body.children != null && body.children.length === 0) {
        return [];
      }
      return [{ label: entry.label, icon: config.icon, ...body }];
    });
  });

  const groups = computed<PaletteGroup[]>(() => {
    const result: PaletteGroup[] = [];
    if (actionItems.value.length > 0) {
      result.push({ id: 'actions', label: 'Actions', items: actionItems.value });
    }
    result.push({ id: 'navigation', label: 'Navigation', items: navigationItems.value });
    return result;
  });

  return { groups };
}

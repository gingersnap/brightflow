/**
 * A source's saved views on the colada cache, and the three edits to them
 * dispatched through the action bus with the standard toast-and-Undo
 * feedback. The list refetches after each edit and on every applied or
 * undone action, since an undo elsewhere can bring a view back.
 */

import { useQuery } from '@pinia/colada';
import { computed, type Ref } from 'vue';

import { useInsightActions } from '@/composables/useInsightActions';
import { viewsApi } from '@/services/api';
import { useCurationStore } from '@/stores/curation';
import type { SavedViewResponse } from '@/types/generated';

export function useSavedViews(sourceId: Ref<string> | (() => string)) {
  const curation = useCurationStore();
  curation.initRealtime();
  const { dispatchWithFeedback } = useInsightActions();
  const id = typeof sourceId === 'function' ? sourceId : (): string => sourceId.value;

  const { data, refetch } = useQuery({
    key: () => ['saved-views', id(), curation.version],
    query: async () => (await viewsApi.list(id())) ?? [],
    enabled: () => id() !== '',
  });
  const views = computed<SavedViewResponse[]>(() => data.value ?? []);

  function forTable(table: string): SavedViewResponse[] {
    return views.value.filter((v) => v.table === table);
  }

  /** Create, or overwrite when `viewId` is given. True when it applied. */
  async function save(input: {
    table: string;
    name: string;
    spec: unknown;
    viewId?: string;
  }): Promise<boolean> {
    const ok = await dispatchWithFeedback(
      {
        kind: 'save_view',
        name: input.name,
        source_id: id(),
        spec: input.spec,
        table: input.table,
        ...(input.viewId == null ? {} : { view_id: input.viewId }),
      },
      { description: input.name, title: input.viewId == null ? 'View saved' : 'View updated' },
    );
    await refetch();
    return ok;
  }

  async function rename(view: SavedViewResponse, name: string): Promise<void> {
    await dispatchWithFeedback(
      { kind: 'rename_view', name, source_id: id(), table: view.table, view_id: view.id },
      { description: `${view.name} → ${name}`, title: 'View renamed' },
    );
    await refetch();
  }

  async function remove(view: SavedViewResponse): Promise<void> {
    await dispatchWithFeedback(
      { kind: 'delete_view', source_id: id(), table: view.table, view_id: view.id },
      { description: view.name, title: 'View deleted' },
    );
    await refetch();
  }

  return { forTable, refetch, remove, rename, save, views };
}

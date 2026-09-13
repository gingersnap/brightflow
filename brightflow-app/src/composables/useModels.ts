/**
 * A source's models on the colada cache, and the four edits to them
 * dispatched through the action bus with the standard toast-and-Undo
 * feedback. The list refetches after each edit and on every applied or
 * undone action; a create or delete also invalidates the source list, since
 * that is where every table picker reads the model's table from.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, type Ref } from 'vue';

import { useInsightActions } from '@/composables/useInsightActions';
import { UNIFIED_SOURCES_KEY } from '@/composables/useSources';
import { modelsApi } from '@/services/api';
import { useCurationStore } from '@/stores/curation';
import type { ModelRecipe, ModelSummary } from '@/types/generated';

export function useModels(sourceId: Ref<string> | (() => string)) {
  const curation = useCurationStore();
  curation.initRealtime();
  const { dispatchWithFeedback } = useInsightActions();
  const queryCache = useQueryCache();
  const id = typeof sourceId === 'function' ? sourceId : (): string => sourceId.value;

  const { data, refetch } = useQuery({
    key: () => ['models', id(), curation.version],
    query: async () => (await modelsApi.list(id())) ?? [],
    enabled: () => id() !== '',
  });
  const models = computed<ModelSummary[]>(() => data.value ?? []);

  function forTable(table: string): ModelSummary | undefined {
    return models.value.find((m) => m.table === table);
  }

  async function tablesChanged(): Promise<void> {
    await Promise.all([
      refetch(),
      queryCache.invalidateQueries({ key: UNIFIED_SOURCES_KEY }),
      queryCache.invalidateQueries({ key: ['tables-index', id()] }),
    ]);
  }

  /** Create a model over `input`. True when it applied. */
  async function create(input: {
    input: string;
    name: string;
    recipe: ModelRecipe;
    clientSpec?: unknown;
  }): Promise<boolean> {
    const ok = await dispatchWithFeedback(
      {
        kind: 'create_model',
        name: input.name,
        recipe: input.recipe,
        source_id: id(),
        table: input.input,
        ...(input.clientSpec == null ? {} : { client_spec: input.clientSpec }),
      },
      { description: `from ${input.input}`, title: `Model ${input.name} built` },
    );
    await tablesChanged();
    return ok;
  }

  async function update(
    model: ModelSummary,
    recipe: ModelRecipe,
    clientSpec?: unknown,
  ): Promise<boolean> {
    const ok = await dispatchWithFeedback(
      {
        kind: 'update_model',
        model_id: model.id,
        recipe,
        source_id: id(),
        table: model.table,
        ...(clientSpec == null ? {} : { client_spec: clientSpec }),
      },
      { description: model.table, title: 'Model rebuilt with the new recipe' },
    );
    await refetch();
    return ok;
  }

  async function rebuild(model: ModelSummary): Promise<void> {
    await dispatchWithFeedback(
      { kind: 'rebuild_model', model_id: model.id, source_id: id(), table: model.table },
      { description: model.table, title: 'Model rebuilt' },
    );
    await refetch();
  }

  async function remove(model: ModelSummary): Promise<void> {
    await dispatchWithFeedback(
      { kind: 'delete_model', model_id: model.id, source_id: id(), table: model.table },
      { description: model.table, title: 'Model deleted' },
    );
    await tablesChanged();
  }

  return { create, forTable, models, rebuild, refetch, remove, update };
}

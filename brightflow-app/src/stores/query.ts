/**
 * Query-builder state for the Explore tool.
 *
 * Holds one section of state per builder panel (filter, sort, limit) and
 * derives `operations`, the wire format the backend executes, honouring each
 * section's `enabled` flag. Pivot state lives in `stores/pivot.ts`.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import type { Filter, QuerySections } from '@/types';
import type { Operation } from '@/types/generated';

type SectionKey = keyof QuerySections;

/**
 * Map filters to wire filter operations. Null-check operators force the value
 * to null so a stale input value can't leak into the query. Shared with
 * useWsQuery so table and pivot paths can't drift.
 */
export function filterOperations(filters: Filter[]): Operation[] {
  const ops: Operation[] = [];
  for (const filter of filters) {
    if (filter.column != null && filter.op !== '') {
      ops.push({
        column: filter.column,
        op: filter.op,
        type: 'filter',
        value: ['isNull', 'isNotNull'].includes(filter.op) ? null : filter.value,
      });
    }
  }
  return ops;
}

export const useQueryStore = defineStore('query', () => {
  // Section states
  const sections = ref<QuerySections>({
    filter: { enabled: true, collapsed: false },
    limit: { enabled: true, collapsed: false },
    sort: { enabled: false, collapsed: true },
  });

  // Filter state
  const filters = ref<Filter[]>([]);

  // Sort state
  const sortBy = ref<string | null>(null);
  const sortDescending = ref(false);

  // Limit state (default 100 for table view)
  const limit = ref(100);

  // Computed: Build operations array for API
  const operations = computed((): Operation[] => {
    const ops: Operation[] = [];

    // Add filters
    if (sections.value.filter.enabled) {
      ops.push(...filterOperations(filters.value));
    }

    // Add sort
    if (sections.value.sort.enabled && sortBy.value != null) {
      ops.push({
        by: sortBy.value,
        descending: sortDescending.value,
        type: 'sort',
      });
    }

    // Add limit
    if (sections.value.limit.enabled && limit.value > 0) {
      ops.push({
        n: limit.value,
        type: 'limit',
      });
    }

    return ops;
  });

  // Actions
  function toggleSection(section: SectionKey): void {
    sections.value[section].enabled = !sections.value[section].enabled;
  }

  function addFilter(): void {
    filters.value.push({
      column: null,
      id: crypto.randomUUID(),
      op: 'eq',
      value: null,
    });
  }

  function updateFilter(id: string, updates: Partial<Filter>): void {
    const filter = filters.value.find((f) => f.id === id);
    if (filter) {
      Object.assign(filter, updates);
    }
  }

  function removeFilter(id: string): void {
    filters.value = filters.value.filter((f) => f.id !== id);
  }

  function reset(): void {
    filters.value = [];
    sortBy.value = null;
    sortDescending.value = false;
    limit.value = 100;

    // Reset section states
    sections.value = {
      filter: { enabled: true, collapsed: false },
      limit: { enabled: true, collapsed: false },
      sort: { enabled: false, collapsed: true },
    };
  }

  return {
    // State
    sections,
    filters,
    sortBy,
    sortDescending,
    limit,
    // Computed
    operations,
    // Actions
    toggleSection,
    addFilter,
    updateFilter,
    removeFilter,
    reset,
  };
});

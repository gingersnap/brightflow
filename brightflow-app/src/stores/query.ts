/**
 * Query-builder state for the Explore tool.
 *
 * Holds one section of state per builder panel (filter, select, groupBy,
 * pivot, sort, limit) and derives two views of it: `operations`, the wire
 * format the backend executes, and `previewTexts`, the collapsed-panel
 * summaries. The two differ deliberately — `operations` honours each section's
 * `enabled` flag, while `previewTexts` reflects raw state so a collapsed or
 * disabled panel still shows what it holds.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import type { Aggregation, Filter, PivotState, QuerySections } from '@/types';
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
    groupBy: { enabled: false, collapsed: true },
    limit: { enabled: true, collapsed: false },
    pivot: { enabled: false, collapsed: true },
    select: { enabled: false, collapsed: true },
    sort: { enabled: false, collapsed: true },
  });

  // Filter state
  const filters = ref<Filter[]>([]);

  // Select state
  const selectedColumns = ref<string[]>([]);

  // Group by state
  const groupByColumns = ref<string[]>([]);
  const aggregations = ref<Aggregation[]>([]);

  // Pivot state
  const pivot = ref<PivotState>({
    agg: 'count',
    columns: null,
    index: [],
    values: null,
  });

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

    // Add group by
    if (sections.value.groupBy.enabled && groupByColumns.value.length > 0) {
      ops.push({
        aggs: aggregations.value.map((agg) => ({
          column: agg.column,
          function: agg.function,
          alias: agg.alias || `${agg.function}_${agg.column}`,
        })),
        by: groupByColumns.value,
        type: 'groupBy',
      });
    }

    // Add pivot
    if (sections.value.pivot.enabled && pivot.value.values != null && pivot.value.columns != null) {
      ops.push({
        agg: pivot.value.agg,
        columns: pivot.value.columns,
        index: pivot.value.index,
        type: 'pivot',
        values: pivot.value.values,
      });
    }

    // Add select
    if (sections.value.select.enabled && selectedColumns.value.length > 0) {
      ops.push({
        columns: selectedColumns.value,
        type: 'select',
      });
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

  // Computed: Preview texts for each section
  const previewTexts = computed(() => ({
    filter:
      filters.value.length > 0
        ? `${filters.value.length} filter${filters.value.length > 1 ? 's' : ''}`
        : 'No filters',
    groupBy:
      groupByColumns.value.length > 0 ? `By: ${groupByColumns.value.join(', ')}` : 'Not grouped',
    limit: `${limit.value.toLocaleString()} rows`,
    pivot:
      pivot.value.values == null
        ? 'Not configured'
        : `${pivot.value.index.length} rows, ${pivot.value.columns ?? 'no'} columns`,
    select:
      selectedColumns.value.length > 0 ? `${selectedColumns.value.length} columns` : 'All columns',
    sort:
      sortBy.value == null
        ? 'Not sorted'
        : `${sortBy.value} ${sortDescending.value ? 'DESC' : 'ASC'}`,
  }));

  const isValid = computed(() => true);

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
    selectedColumns.value = [];
    groupByColumns.value = [];
    aggregations.value = [];
    pivot.value = { agg: 'count', columns: null, index: [], values: null };
    sortBy.value = null;
    sortDescending.value = false;
    limit.value = 100;

    // Reset section states
    sections.value = {
      filter: { enabled: true, collapsed: false },
      groupBy: { enabled: false, collapsed: true },
      limit: { enabled: true, collapsed: false },
      pivot: { enabled: false, collapsed: true },
      select: { enabled: false, collapsed: true },
      sort: { enabled: false, collapsed: true },
    };
  }

  return {
    // State
    sections,
    filters,
    selectedColumns,
    groupByColumns,
    aggregations,
    pivot,
    sortBy,
    sortDescending,
    limit,
    // Computed
    operations,
    previewTexts,
    isValid,
    // Actions
    toggleSection,
    addFilter,
    updateFilter,
    removeFilter,
    reset,
  };
});

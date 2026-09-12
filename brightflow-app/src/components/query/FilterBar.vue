<script setup lang="ts">
/**
 * The "Filters & Options" section of the query builder: filter chips
 * (column / operator / value, with operators chosen per column type) plus
 * the row limit. Changes re-run the table query through a 300ms debounce,
 * and only while connected with data loaded.
 */

import { watchDebounced } from '@vueuse/core';
import { computed } from 'vue';

import CollapsibleSection from '@/components/common/CollapsibleSection.vue';
import { useOperators } from '@/composables/useOperators';
import { useWsQuery } from '@/composables/useWsQuery';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { useQueryStore } from '@/stores/query';
import { useUiStore } from '@/stores/ui';
import type { Filter, Operator } from '@/types';
import type { LogicalType } from '@/types/generated';

const queryStore = useQueryStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();
const connectionStore = useConnectionStore();
const { getOperatorsForType, operatorNeedsValue, getDefaultOperator, isFilterOp } = useOperators();
const { loadTableData } = useWsQuery();

const filtersOpen = computed({
  get: () => !uiStore.filterCollapsed,
  set: () => uiStore.toggleSection('filter'),
});

// Limit options
const limitOptions = [
  { label: '100', value: 100 },
  { label: '500', value: 500 },
  { label: '1,000', value: 1000 },
  { label: '5,000', value: 5000 },
  { label: 'All', value: 0 },
];

// Re-query table data when filters or limit change (debounced; cleans up on unmount)
watchDebounced(
  () => [queryStore.filters.map((f) => `${f.column}:${f.op}:${f.value}`), queryStore.limit],
  () => {
    if (connectionStore.isConnected && datasetStore.hasData) {
      loadTableData();
    }
  },
  { debounce: 300, deep: true },
);

// Column options for dropdown: visible columns, shown by label
const columnOptions = computed(() =>
  datasetStore.visibleColumns.map((col) => ({
    label: datasetStore.labelFor(col.name),
    value: col.name,
    datatype: col.datatype,
  })),
);

// The column's logical type; an unknown column is treated as text
function getColumnType(columnName: string): LogicalType {
  const col = datasetStore.columns.find((c) => c.name === columnName);
  return col?.datatype ?? 'String';
}

// Get operators for a filter's column
function getOperators(filter: Filter): Operator[] {
  if (!filter.column) {
    return [];
  }
  return getOperatorsForType(getColumnType(filter.column));
}

// Handle column change
function handleColumnChange(filterId: string, columnName: string): void {
  const defaultOp = getDefaultOperator(getColumnType(columnName));
  queryStore.updateFilter(filterId, {
    column: columnName,
    op: defaultOp,
    value: null,
  });
}

// Handle operator change. The guard narrows the select's string value.
// Items come from OPERATORS, so the else branch is unreachable in practice.
function handleOperatorChange(filterId: string, op: string): void {
  if (isFilterOp(op)) {
    queryStore.updateFilter(filterId, { op, value: null });
  }
}

// Handle value change
function handleValueChange(filterId: string, value: unknown): void {
  queryStore.updateFilter(filterId, { value });
}

// Check if filters are enabled (has active filters)
const hasActiveFilters = computed(() => queryStore.filters.some((f) => f.column && f.op));
</script>

<template>
  <CollapsibleSection v-model:open="filtersOpen" class="border-b border-default">
    <template #title>
      <h2 class="text-sm font-medium text-default">Filters & Options</h2>
      <span v-if="hasActiveFilters" class="text-xs text-muted">
        ({{ queryStore.filters.filter((f) => f.column).length }} active)
      </span>
    </template>

    <!-- Content -->
    <div class="space-y-2 border-t border-default bg-muted/10 px-4 py-2">
      <!-- Filters Row -->
      <div class="flex flex-wrap items-center gap-2">
        <span class="w-12 text-sm font-medium text-muted">Filter:</span>
        <div
          v-for="filter in queryStore.filters"
          :key="filter.id"
          class="group flex items-center gap-1 rounded-md border border-default bg-default py-1 pr-1 pl-2 text-xs"
        >
          <!-- Column selector -->
          <USelectMenu
            :model-value="filter.column ?? ''"
            :items="columnOptions"
            placeholder="Column"
            value-key="value"
            size="xs"
            variant="none"
            class="min-w-20"
            @update:model-value="(val: string) => handleColumnChange(filter.id, val)"
          />

          <!-- Operator selector -->
          <USelectMenu
            :model-value="filter.op"
            :items="getOperators(filter)"
            value-key="value"
            size="xs"
            variant="none"
            class="w-16"
            :disabled="!filter.column"
            @update:model-value="(val: string) => handleOperatorChange(filter.id, val)"
          />

          <!-- Value input -->
          <UInput
            v-if="operatorNeedsValue(filter.op)"
            :model-value="(filter.value as string | number | null) ?? ''"
            placeholder="value"
            size="xs"
            variant="none"
            class="w-24"
            :disabled="!filter.column"
            @update:model-value="(val: string | number) => handleValueChange(filter.id, val)"
          />

          <!-- Remove button -->
          <button
            class="rounded p-0.5 text-muted transition-colors hover:bg-muted/50 hover:text-default"
            @click="queryStore.removeFilter(filter.id)"
          >
            <UIcon name="i-lucide-x" class="h-3 w-3" />
          </button>
        </div>

        <!-- Add filter button -->
        <button
          class="flex items-center gap-1 rounded-md px-2 py-1 text-sm text-muted transition-colors hover:bg-muted/30 hover:text-default"
          @click="queryStore.addFilter()"
        >
          <UIcon name="i-lucide-plus" class="h-3.5 w-3.5" />
          <span>Add</span>
        </button>
      </div>

      <!-- Limit Row -->
      <div class="flex items-center gap-2">
        <span class="w-12 text-sm font-medium text-muted">Limit:</span>
        <USelectMenu
          :model-value="queryStore.limit"
          :items="limitOptions"
          value-key="value"
          size="xs"
          class="w-20"
          @update:model-value="queryStore.limit = $event"
        />
        <span class="text-sm text-muted">rows</span>
      </div>
    </div>
  </CollapsibleSection>
</template>

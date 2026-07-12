<script setup lang="ts">
import { ChevronDown, ChevronRight, Plus, X } from '@lucide/vue';
import { computed, watch } from 'vue';

import { useOperators } from '@/composables/useOperators';
import { useWsQuery } from '@/composables/useWsQuery';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { useQueryStore } from '@/stores/query';
import { useUiStore } from '@/stores/ui';
import type { Filter, Operator } from '@/types';

const queryStore = useQueryStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();
const connectionStore = useConnectionStore();
const { getOperatorsForType, operatorNeedsValue, getDefaultOperator } = useOperators();
const { loadTableData } = useWsQuery();

const isCollapsed = computed(() => uiStore.filterCollapsed);

// Limit options
const limitOptions = [
  { label: '100', value: 100 },
  { label: '500', value: 500 },
  { label: '1,000', value: 1000 },
  { label: '5,000', value: 5000 },
  { label: 'All', value: 0 },
];

// Re-query table data when filters or limit change
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
watch(
  () => [queryStore.filters.map((f) => `${f.column}:${f.op}:${f.value}`), queryStore.limit],
  () => {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(() => {
      if (connectionStore.isConnected && datasetStore.hasData) {
        loadTableData();
      }
    }, 300);
  },
  { deep: true },
);

// Column options for dropdown
const columnOptions = computed(() =>
  datasetStore.columns.map((col) => ({
    label: col.name,
    value: col.name,
    dtype: col.dtype,
  })),
);

// Get dtype for a column
function getColumnDtype(columnName: string): string {
  const col = datasetStore.columns.find((c) => c.name === columnName);
  return col?.dtype ?? 'string';
}

// Get operators for a filter's column
function getOperators(filter: Filter): Operator[] {
  if (!filter.column) {
    return [];
  }
  const dtype = getColumnDtype(filter.column);
  return getOperatorsForType(dtype);
}

// Handle column change
function handleColumnChange(filterId: string, columnName: string): void {
  const dtype = getColumnDtype(columnName);
  const defaultOp = getDefaultOperator(dtype);
  queryStore.updateFilter(filterId, {
    column: columnName,
    op: defaultOp,
    value: null,
  });
}

// Handle operator change
function handleOperatorChange(filterId: string, op: string): void {
  queryStore.updateFilter(filterId, { op, value: null });
}

// Handle value change
function handleValueChange(filterId: string, value: unknown): void {
  queryStore.updateFilter(filterId, { value });
}

// Check if filters are enabled (has active filters)
const hasActiveFilters = computed(() => queryStore.filters.some((f) => f.column && f.op));
</script>

<template>
  <div class="border-b border-default">
    <!-- Section Header -->
    <button
      class="flex w-full items-center gap-2 bg-muted/30 px-4 py-2 text-left transition-colors hover:bg-muted/40"
      @click="uiStore.toggleSection('filter')"
    >
      <component :is="isCollapsed ? ChevronRight : ChevronDown" class="h-4 w-4 text-muted" />
      <h2 class="text-sm font-medium text-default">Filters & Options</h2>
      <span v-if="hasActiveFilters" class="text-xs text-muted">
        ({{ queryStore.filters.filter((f) => f.column).length }} active)
      </span>
    </button>

    <!-- Content -->
    <div v-if="!isCollapsed" class="space-y-2 border-t border-default bg-muted/10 px-4 py-2">
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
            <X class="h-3 w-3" />
          </button>
        </div>

        <!-- Add filter button -->
        <button
          class="flex items-center gap-1 rounded-md px-2 py-1 text-sm text-muted transition-colors hover:bg-muted/30 hover:text-default"
          @click="queryStore.addFilter()"
        >
          <Plus class="h-3.5 w-3.5" />
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
  </div>
</template>

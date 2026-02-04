<script setup lang="ts">
import { computed, watch } from 'vue'
import { Plus, X, ChevronRight, ChevronDown } from 'lucide-vue-next'
import { useQueryStore } from '@/stores/query'
import { useDatasetStore } from '@/stores/dataset'
import { useUiStore } from '@/stores/ui'
import { useConnectionStore } from '@/stores/connection'
import { useOperators } from '@/composables/useOperators'
import { useQuery } from '@/composables/useQuery'
import type { Filter, Operator } from '@/types'

const queryStore = useQueryStore()
const datasetStore = useDatasetStore()
const uiStore = useUiStore()
const connectionStore = useConnectionStore()
const { getOperatorsForType, operatorNeedsValue, getDefaultOperator } = useOperators()
const { loadTableData } = useQuery()

const isCollapsed = computed(() => uiStore.filterCollapsed)

// Limit options
const limitOptions = [
  { label: '100', value: 100 },
  { label: '500', value: 500 },
  { label: '1,000', value: 1000 },
  { label: '5,000', value: 5000 },
  { label: 'All', value: 0 }
]

// Re-query table data when filters or limit change
let debounceTimer: ReturnType<typeof setTimeout> | null = null
watch(
  () => [
    queryStore.filters.map(f => `${f.column}:${f.op}:${f.value}`),
    queryStore.limit
  ],
  () => {
    if (debounceTimer) clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      if (connectionStore.isConnected && datasetStore.hasData) {
        loadTableData()
      }
    }, 300)
  },
  { deep: true }
)

// Column options for dropdown
const columnOptions = computed(() => {
  return datasetStore.columns.map(col => ({
    label: col.name,
    value: col.name,
    dtype: col.dtype
  }))
})

// Get dtype for a column
function getColumnDtype(columnName: string): string {
  const col = datasetStore.columns.find(c => c.name === columnName)
  return col?.dtype ?? 'string'
}

// Get operators for a filter's column
function getOperators(filter: Filter): Operator[] {
  if (!filter.column) return []
  const dtype = getColumnDtype(filter.column)
  return getOperatorsForType(dtype)
}


// Handle column change
function handleColumnChange(filterId: string, columnName: string): void {
  const dtype = getColumnDtype(columnName)
  const defaultOp = getDefaultOperator(dtype)
  queryStore.updateFilter(filterId, {
    column: columnName,
    op: defaultOp,
    value: null
  })
}

// Handle operator change
function handleOperatorChange(filterId: string, op: string): void {
  queryStore.updateFilter(filterId, { op, value: null })
}

// Handle value change
function handleValueChange(filterId: string, value: unknown): void {
  queryStore.updateFilter(filterId, { value })
}

// Check if filters are enabled (has active filters)
const hasActiveFilters = computed(() =>
  queryStore.filters.some(f => f.column && f.op)
)
</script>

<template>
  <div class="border-b border-default">
    <!-- Section Header -->
    <button
      class="flex items-center gap-2 w-full px-4 py-2 bg-muted/30 text-left hover:bg-muted/40 transition-colors"
      @click="uiStore.toggleSection('filter')"
    >
      <component
        :is="isCollapsed ? ChevronRight : ChevronDown"
        class="w-4 h-4 text-muted"
      />
      <h2 class="text-sm font-medium text-default">Filters & Options</h2>
      <span v-if="hasActiveFilters" class="text-xs text-muted">
        ({{ queryStore.filters.filter(f => f.column).length }} active)
      </span>
    </button>

    <!-- Content -->
    <div v-if="!isCollapsed" class="px-4 py-2 bg-muted/10 border-t border-default space-y-2">
      <!-- Filters Row -->
      <div class="flex items-center gap-2 flex-wrap">
        <span class="text-xs text-muted font-medium w-12">Filter:</span>
        <div
          v-for="filter in queryStore.filters"
          :key="filter.id"
          class="flex items-center gap-1 pl-2 pr-1 py-1 rounded-md bg-default border border-default text-xs group"
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
            class="p-0.5 rounded hover:bg-muted/50 text-muted hover:text-default transition-colors"
            @click="queryStore.removeFilter(filter.id)"
          >
            <X class="w-3 h-3" />
          </button>
        </div>

        <!-- Add filter button -->
        <button
          class="flex items-center gap-1 px-2 py-1 text-xs text-muted hover:text-default hover:bg-muted/30 rounded-md transition-colors"
          @click="queryStore.addFilter()"
        >
          <Plus class="w-3.5 h-3.5" />
          <span>Add</span>
        </button>
      </div>

      <!-- Limit Row -->
      <div class="flex items-center gap-2">
        <span class="text-xs text-muted font-medium w-12">Limit:</span>
        <USelectMenu
          :model-value="queryStore.limit"
          :items="limitOptions"
          value-key="value"
          size="xs"
          class="w-20"
          @update:model-value="queryStore.limit = $event"
        />
        <span class="text-xs text-muted">rows</span>
      </div>
    </div>
  </div>
</template>

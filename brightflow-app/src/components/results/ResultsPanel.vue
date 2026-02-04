<script setup lang="ts">
import { computed, type Component } from 'vue'
import { Table, BarChart3, Split, Download, Loader2, TableProperties, ChevronRight, ChevronDown, Hash } from 'lucide-vue-next'
import { useResultsStore } from '@/stores/results'
import { useUiStore } from '@/stores/ui'
import { usePivotStore } from '@/stores/pivot'
import DataTable from './DataTable.vue'
import ChartView from '../charts/ChartView.vue'
import PivotTable from '../pivot/PivotTable.vue'
import BigNumber from '../charts/BigNumber.vue'
import type { ViewMode } from '@/types'

const resultsStore = useResultsStore()
const uiStore = useUiStore()
const pivotStore = usePivotStore()

const isCollapsed = computed(() => uiStore.resultsCollapsed)

interface ViewModeOption {
  value: ViewMode
  label: string
  icon: Component
}

const viewModes: ViewModeOption[] = [
  { value: 'table', label: 'Table', icon: Table },
  { value: 'pivot', label: 'Pivot', icon: TableProperties },
  { value: 'number', label: 'Number', icon: Hash },
  { value: 'chart', label: 'Chart', icon: BarChart3 },
  { value: 'split', label: 'Split', icon: Split }
]

// Row count based on view mode
const currentRowCount = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.pivotRowCount
  }
  return resultsStore.tableRowCount
})

// Total rows (before limit) - will be used when backend supports it
const currentTotalRows = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.pivotTotalRows
  }
  return resultsStore.tableTotalRows
})

// Execution time based on view mode
const currentExecutionTime = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.pivotExecutionTimeMs
  }
  return resultsStore.tableExecutionTimeMs
})

// Has results for current view
const hasCurrentResults = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.hasPivotResults
  }
  return resultsStore.hasTableResults
})

// Display text for row count
const rowCountDisplay = computed(() => {
  const count = currentRowCount.value
  const total = currentTotalRows.value

  if (total && total > count) {
    return `Showing ${count.toLocaleString()} of ${total.toLocaleString()}`
  }
  return `${count.toLocaleString()} rows`
})
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Section Header -->
    <div class="flex items-center justify-between bg-muted/30 border-b border-default">
      <button
        class="flex items-center gap-2 px-4 py-2 text-left hover:bg-muted/40 transition-colors"
        @click="uiStore.toggleSection('results')"
      >
        <component
          :is="isCollapsed ? ChevronRight : ChevronDown"
          class="w-4 h-4 text-muted"
        />
        <h2 class="text-sm font-medium text-default">Results</h2>
        <span v-if="hasCurrentResults" class="text-xs text-muted">
          ({{ rowCountDisplay }})
        </span>
      </button>

      <div v-if="!isCollapsed" class="flex items-center gap-4 pr-4">
        <!-- View mode toggle -->
        <div class="flex gap-1" @click.stop>
          <UButton
            v-for="mode in viewModes"
            :key="mode.value"
            :variant="uiStore.viewMode === mode.value ? 'solid' : 'ghost'"
            :color="uiStore.viewMode === mode.value ? 'primary' : 'neutral'"
            size="xs"
            @click="uiStore.setViewMode(mode.value)"
          >
            <component :is="mode.icon" class="w-3.5 h-3.5" />
          </UButton>
        </div>

        <!-- Status -->
        <div v-if="hasCurrentResults && currentExecutionTime" class="text-xs text-muted">
          {{ currentExecutionTime.toFixed(1) }}ms
        </div>

        <!-- Export -->
        <UButton
          v-if="hasCurrentResults"
          variant="ghost"
          color="neutral"
          size="xs"
          @click.stop="resultsStore.exportCsv(uiStore.viewMode === 'pivot' ? 'pivot' : 'table')"
        >
          <Download class="w-3.5 h-3.5 mr-1" />
          Export
        </UButton>
      </div>
    </div>

    <!-- Content -->
    <div v-if="!isCollapsed" class="flex-1 overflow-hidden relative">
      <!-- Loading overlay -->
      <div
        v-if="resultsStore.loading"
        class="absolute inset-0 bg-default/80 flex items-center justify-center z-10"
      >
        <div class="flex items-center gap-2 text-muted">
          <Loader2 class="w-5 h-5 animate-spin" />
          <span>Executing query...</span>
        </div>
      </div>

      <!-- Error state -->
      <div
        v-else-if="resultsStore.error"
        class="flex items-center justify-center h-full"
      >
        <div class="text-center p-8 max-w-md">
          <div class="text-red-500 text-sm font-medium mb-2">Query Error</div>
          <div class="text-muted text-sm">{{ resultsStore.error }}</div>
        </div>
      </div>

      <!-- Pivot view -->
      <template v-if="uiStore.viewMode === 'pivot'">
        <div v-if="resultsStore.hasPivotResults && pivotStore.isConfigured" class="h-full overflow-hidden">
          <PivotTable />
        </div>
        <div v-else class="flex items-center justify-center h-full">
          <div class="text-center p-8">
            <TableProperties class="w-12 h-12 text-muted/50 mx-auto mb-4" />
            <div class="text-muted">Configure your pivot table</div>
            <div class="text-sm text-muted/70 mt-1">
              Drag columns into Values to create a pivot
            </div>
          </div>
        </div>
      </template>

      <!-- Table view -->
      <template v-else-if="uiStore.viewMode === 'table'">
        <DataTable v-if="resultsStore.hasTableResults" class="h-full" />
        <div v-else class="flex items-center justify-center h-full">
          <div class="text-center p-8">
            <Table class="w-12 h-12 text-muted/50 mx-auto mb-4" />
            <div class="text-muted">No data loaded</div>
            <div class="text-sm text-muted/70">Upload a file to see data</div>
          </div>
        </div>
      </template>

      <!-- Number view (BigNumber) -->
      <template v-else-if="uiStore.viewMode === 'number'">
        <BigNumber v-if="resultsStore.hasResults" class="h-full" />
        <div v-else class="flex items-center justify-center h-full">
          <div class="text-center p-8">
            <Hash class="w-12 h-12 text-muted/50 mx-auto mb-4" />
            <div class="text-muted">No data for display</div>
          </div>
        </div>
      </template>

      <!-- Chart view -->
      <template v-else-if="uiStore.viewMode === 'chart'">
        <ChartView v-if="resultsStore.hasTableResults" class="h-full" />
        <div v-else class="flex items-center justify-center h-full">
          <div class="text-center p-8">
            <BarChart3 class="w-12 h-12 text-muted/50 mx-auto mb-4" />
            <div class="text-muted">No data for chart</div>
          </div>
        </div>
      </template>

      <!-- Split view -->
      <template v-else-if="uiStore.viewMode === 'split'">
        <div v-if="resultsStore.hasTableResults" class="flex h-full">
          <DataTable class="w-1/2 border-r border-default" />
          <ChartView class="w-1/2" />
        </div>
        <div v-else class="flex items-center justify-center h-full">
          <div class="text-center p-8">
            <Split class="w-12 h-12 text-muted/50 mx-auto mb-4" />
            <div class="text-muted">No data loaded</div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

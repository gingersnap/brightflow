<script setup lang="ts">
import {
  BarChart3,
  ChevronDown,
  ChevronRight,
  Download,
  Hash,
  Loader2,
  Split,
  Table,
  TableProperties,
} from 'lucide-vue-next';
import { type Component, computed } from 'vue';

import { usePivotStore } from '@/stores/pivot';
import { useResultsStore } from '@/stores/results';
import { useUiStore } from '@/stores/ui';
import type { ViewMode } from '@/types';

import BigNumber from '../charts/BigNumber.vue';
import ChartView from '../charts/ChartView.vue';
import PivotTable from '../pivot/PivotTable.vue';
import DataTable from './DataTable.vue';

const resultsStore = useResultsStore();
const uiStore = useUiStore();
const pivotStore = usePivotStore();

const isCollapsed = computed(() => uiStore.resultsCollapsed);

interface ViewModeOption {
  value: ViewMode;
  label: string;
  icon: Component;
}

const viewModes: ViewModeOption[] = [
  { icon: Table, label: 'Table', value: 'table' },
  { icon: TableProperties, label: 'Pivot', value: 'pivot' },
  { icon: Hash, label: 'Number', value: 'number' },
  { icon: BarChart3, label: 'Chart', value: 'chart' },
  { icon: Split, label: 'Split', value: 'split' },
];

// Row count based on view mode
const currentRowCount = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.pivotRowCount;
  }
  return resultsStore.tableRowCount;
});

// Total rows (before limit) - will be used when backend supports it
const currentTotalRows = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.pivotTotalRows;
  }
  return resultsStore.tableTotalRows;
});

// Execution time based on view mode
const currentExecutionTime = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.pivotExecutionTimeMs;
  }
  return resultsStore.tableExecutionTimeMs;
});

// Has results for current view
const hasCurrentResults = computed(() => {
  if (uiStore.viewMode === 'pivot') {
    return resultsStore.hasPivotResults;
  }
  return resultsStore.hasTableResults;
});

// Display text for row count
const rowCountDisplay = computed(() => {
  const count = currentRowCount.value;
  const total = currentTotalRows.value;

  if (total && total > count) {
    return `Showing ${count.toLocaleString()} of ${total.toLocaleString()}`;
  }
  return `${count.toLocaleString()} rows`;
});
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Section Header -->
    <div class="flex items-center justify-between border-b border-default bg-muted/30">
      <button
        class="flex items-center gap-2 px-4 py-2 text-left transition-colors hover:bg-muted/40"
        @click="uiStore.toggleSection('results')"
      >
        <component :is="isCollapsed ? ChevronRight : ChevronDown" class="h-4 w-4 text-muted" />
        <h2 class="text-sm font-medium text-default">Results</h2>
        <span v-if="hasCurrentResults" class="text-xs text-muted"> ({{ rowCountDisplay }}) </span>
      </button>

      <div v-if="!isCollapsed" class="flex items-center gap-4 pr-4">
        <!-- View mode toggle -->
        <div class="flex gap-1" @click.stop>
          <UButton
            v-for="mode in viewModes"
            :key="mode.value"
            :variant="uiStore.viewMode === mode.value ? 'solid' : 'ghost'"
            :color="uiStore.viewMode === mode.value ? 'primary' : 'neutral'"
            size="md"
            @click="uiStore.setViewMode(mode.value)"
          >
            <component :is="mode.icon" class="h-3.5 w-3.5" />
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
          size="md"
          @click.stop="resultsStore.exportCsv(uiStore.viewMode === 'pivot' ? 'pivot' : 'table')"
        >
          <Download class="mr-1 h-3.5 w-3.5" />
          Export
        </UButton>
      </div>
    </div>

    <!-- Content -->
    <div v-if="!isCollapsed" class="relative flex-1 overflow-hidden">
      <!-- Loading overlay -->
      <div
        v-if="resultsStore.loading"
        class="absolute inset-0 z-10 flex items-center justify-center bg-default/80"
      >
        <div class="flex items-center gap-2 text-muted">
          <Loader2 class="h-5 w-5 animate-spin" />
          <span>Executing query...</span>
        </div>
      </div>

      <!-- Error state -->
      <div v-else-if="resultsStore.error" class="flex h-full items-center justify-center">
        <div class="max-w-md p-8 text-center">
          <div class="mb-2 text-sm font-medium text-red-500">Query Error</div>
          <div class="text-sm text-muted">{{ resultsStore.error }}</div>
        </div>
      </div>

      <!-- Pivot view -->
      <template v-if="uiStore.viewMode === 'pivot'">
        <div
          v-if="resultsStore.hasPivotResults && pivotStore.isConfigured"
          class="h-full overflow-hidden"
        >
          <PivotTable />
        </div>
        <div v-else class="flex h-full items-center justify-center">
          <div class="p-8 text-center">
            <TableProperties class="mx-auto mb-4 h-12 w-12 text-muted/50" />
            <div class="text-muted">Configure your pivot table</div>
            <div class="mt-1 text-sm text-muted/70">Drag columns into Values to create a pivot</div>
          </div>
        </div>
      </template>

      <!-- Table view -->
      <template v-else-if="uiStore.viewMode === 'table'">
        <DataTable v-if="resultsStore.hasTableResults" class="h-full" />
        <div v-else class="flex h-full items-center justify-center">
          <div class="p-8 text-center">
            <Table class="mx-auto mb-4 h-12 w-12 text-muted/50" />
            <div class="text-muted">No data loaded</div>
            <div class="text-sm text-muted/70">Upload a file to see data</div>
          </div>
        </div>
      </template>

      <!-- Number view (BigNumber) -->
      <template v-else-if="uiStore.viewMode === 'number'">
        <BigNumber v-if="resultsStore.hasResults" class="h-full" />
        <div v-else class="flex h-full items-center justify-center">
          <div class="p-8 text-center">
            <Hash class="mx-auto mb-4 h-12 w-12 text-muted/50" />
            <div class="text-muted">No data for display</div>
          </div>
        </div>
      </template>

      <!-- Chart view -->
      <template v-else-if="uiStore.viewMode === 'chart'">
        <ChartView v-if="resultsStore.hasTableResults" class="h-full" />
        <div v-else class="flex h-full items-center justify-center">
          <div class="p-8 text-center">
            <BarChart3 class="mx-auto mb-4 h-12 w-12 text-muted/50" />
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
        <div v-else class="flex h-full items-center justify-center">
          <div class="p-8 text-center">
            <Split class="mx-auto mb-4 h-12 w-12 text-muted/50" />
            <div class="text-muted">No data loaded</div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

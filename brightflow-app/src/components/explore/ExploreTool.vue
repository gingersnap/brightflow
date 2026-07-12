<script setup lang="ts">
import { ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import FilterBar from '@/components/query/FilterBar.vue';
import QueryBuilder from '@/components/query/QueryBuilder.vue';
import ResultsPanel from '@/components/results/ResultsPanel.vue';
import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import { tableApi } from '@/services/api';
import { resetAllStores } from '@/stores';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { useUiStore } from '@/stores/ui';
import type { SourceTable } from '@/types';

const props = defineProps<{
  sourceId: string;
  table?: string;
}>();

const router = useRouter();
const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();

const loadingTable = ref(false);

async function loadTable(name: string): Promise<void> {
  loadingTable.value = true;
  try {
    resetAllStores();
    const result = await tableApi.load(props.sourceId, name);
    if (result) {
      datasetStore.setFromLoadResponse(result);
      connectionStore.connect();
    }
  } finally {
    loadingTable.value = false;
  }
}

function handleSelectTable(table: SourceTable): void {
  router.push({ name: 'explore-table', params: { sourceId: props.sourceId, table: table.name } });
}

function handleAutoSelectTable(table: SourceTable): void {
  router.replace({
    name: 'explore-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

// Watch table prop — load when it changes, clean up on the bare route
watch(
  () => [props.sourceId, props.table] as const,
  ([, name], prev) => {
    if (name) {
      if (prev && prev[1] == null) {
        // Arriving from the bare route: reopen sections to their defaults
        uiStore.filterCollapsed = true;
        uiStore.summarizeCollapsed = false;
        uiStore.resultsCollapsed = false;
      }
      loadTable(name);
    } else {
      connectionStore.disconnect();
      resetAllStores();
      // Downstream sections render greyed out — keep them collapsed too
      uiStore.filterCollapsed = true;
      uiStore.summarizeCollapsed = true;
      uiStore.resultsCollapsed = true;
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full flex-col">
    <TableSectionPane
      :source-id="sourceId"
      :selected-table="table"
      @select-table="handleSelectTable"
      @auto-select-table="handleAutoSelectTable"
    />

    <div :inert="!table" :class="{ 'opacity-50': !table }">
      <FilterBar />
      <QueryBuilder />
    </div>

    <div class="relative min-h-0 flex-1 overflow-hidden">
      <div class="h-full" :inert="!table" :class="{ 'opacity-50': !table }">
        <ResultsPanel />
      </div>
      <div v-if="!table" class="absolute inset-0 flex items-center justify-center bg-default/60">
        <p class="text-sm text-muted">Choose a table above to start exploring</p>
      </div>
      <div
        v-else-if="loadingTable"
        class="absolute inset-0 flex items-center justify-center bg-default/60"
      >
        <p class="text-muted">Loading table...</p>
      </div>
    </div>
  </div>
</template>

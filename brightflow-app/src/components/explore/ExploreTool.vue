<script setup lang="ts">
import { ArrowLeft } from 'lucide-vue-next';
import { ref } from 'vue';

import ExploreTablePicker from '@/components/explore/ExploreTablePicker.vue';
import FilterBar from '@/components/query/FilterBar.vue';
import QueryBuilder from '@/components/query/QueryBuilder.vue';
import ResultsPanel from '@/components/results/ResultsPanel.vue';
import { tableApi } from '@/services/api';
import { resetAllStores } from '@/stores';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { useSourceStore } from '@/stores/source';
import type { SourceTable } from '@/types';

const sourceStore = useSourceStore();
const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();

const loadingTable = ref(false);

async function handleSelectTable(table: SourceTable): Promise<void> {
  loadingTable.value = true;
  try {
    resetAllStores();
    const result = await tableApi.load(table.name);
    if (result) {
      datasetStore.setFromLoadResponse(result);
      connectionStore.connect();
      sourceStore.selectTable(table.name);
    }
  } finally {
    loadingTable.value = false;
  }
}

function handleBackToTables(): void {
  sourceStore.selectTable(null);
  connectionStore.disconnect();
  resetAllStores();
}
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Table picker phase -->
    <template v-if="sourceStore.selectedTableName == null">
      <div v-if="loadingTable" class="flex flex-1 items-center justify-center">
        <p class="text-muted">Loading table...</p>
      </div>
      <ExploreTablePicker v-else @select-table="handleSelectTable" />
    </template>

    <!-- Query builder phase -->
    <template v-else>
      <div class="border-b border-default px-4 py-2">
        <button
          class="flex cursor-pointer items-center gap-1 text-xs text-muted transition-colors hover:text-highlighted"
          @click="handleBackToTables"
        >
          <ArrowLeft class="h-3 w-3" />
          Back to tables
        </button>
      </div>
      <FilterBar />
      <QueryBuilder />
      <div class="min-h-0 flex-1 overflow-hidden">
        <ResultsPanel />
      </div>
    </template>
  </div>
</template>

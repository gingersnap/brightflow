<script setup lang="ts">
import { ArrowLeft } from 'lucide-vue-next';
import { ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import ExploreTablePicker from '@/components/explore/ExploreTablePicker.vue';
import FilterBar from '@/components/query/FilterBar.vue';
import QueryBuilder from '@/components/query/QueryBuilder.vue';
import ResultsPanel from '@/components/results/ResultsPanel.vue';
import { tableApi } from '@/services/api';
import { resetAllStores } from '@/stores';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import type { SourceTable } from '@/types';

const props = defineProps<{
  sourceId: string;
  table?: string;
}>();

const router = useRouter();
const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();

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

function handleBackToTables(): void {
  connectionStore.disconnect();
  resetAllStores();
  router.push({ name: 'source-tool', params: { sourceId: props.sourceId, tool: 'explore' } });
}

// Watch table prop — load when it changes
watch(
  () => props.table,
  (name) => {
    if (name) {
      loadTable(name);
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Table picker phase -->
    <template v-if="!table">
      <div v-if="loadingTable" class="flex flex-1 items-center justify-center">
        <p class="text-muted">Loading table...</p>
      </div>
      <ExploreTablePicker :source-id="sourceId" @select-table="handleSelectTable" />
    </template>

    <!-- Query builder phase -->
    <template v-else>
      <div class="border-b border-default px-4 py-2">
        <button
          class="flex cursor-pointer items-center gap-1 text-sm text-muted transition-colors hover:text-highlighted"
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

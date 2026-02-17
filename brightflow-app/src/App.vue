<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import { useConnectionStore } from './stores/connection';
import { useDatasetStore } from './stores/dataset';
import { useUiStore } from './stores/ui';
import { resetAllStores } from './stores';
import { tableApi, type TableInfo } from './services/api';

import AppHeader from './components/layout/AppHeader.vue';
import FilterBar from './components/query/FilterBar.vue';
import QueryBuilder from './components/query/QueryBuilder.vue';
import ResultsPanel from './components/results/ResultsPanel.vue';
import InsightsView from './components/insights/InsightsView.vue';
import DatasetPickerModal from './components/layout/DatasetPickerModal.vue';

const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();

// Modal controls the UI gate - user must choose a dataset first
const showDatasetPicker = ref(true);
const currentDataset = ref<string | null>(null);
const loadingTable = ref(false);

onMounted(() => {
  console.log('[App] Mounting, connecting to WebSocket...');
  connectionStore.connect();
});

// Handle dataset selection from modal
async function handleDatasetSelect(table: TableInfo): Promise<void> {
  console.log('[App] Selected table:', table.name);
  loadingTable.value = true;

  try {
    // Reset all stores for clean slate
    resetAllStores();

    // Load the selected table via REST API
    const result = await tableApi.load(table.name);

    if (result) {
      // Fetch metadata for the loaded table via WebSocket
      datasetStore.fetchMetadata(result.id);
      currentDataset.value = table.name;
      showDatasetPicker.value = false;
    }
  } catch (e) {
    console.error('[App] Failed to load table:', e);
  } finally {
    loadingTable.value = false;
  }
}

// Open modal to change dataset
function handleChangeDataset(): void {
  showDatasetPicker.value = true;
}

// Wait for connection before allowing interaction
watch(
  () => connectionStore.isConnected,
  (connected) => {
    if (connected && !currentDataset.value) {
      // Connection established but no dataset loaded - modal should be open
      showDatasetPicker.value = true;
    }
  },
);
</script>

<template>
  <UApp>
    <!-- Dataset picker modal - acts as gate until dataset is chosen -->
    <DatasetPickerModal
      :open="showDatasetPicker && connectionStore.isConnected"
      @select="handleDatasetSelect"
    />

    <div class="h-screen flex flex-col bg-default">
      <!-- Header with change dataset action -->
      <AppHeader
        :current-dataset="currentDataset"
        @change-dataset="handleChangeDataset"
      />

      <!-- Main content - only interactive after dataset is loaded -->
      <template v-if="currentDataset">
        <!-- Explore mode -->
        <template v-if="uiStore.appMode === 'explore'">
          <FilterBar />
          <QueryBuilder />
          <div class="flex-1 min-h-0 overflow-hidden">
            <ResultsPanel />
          </div>
        </template>

        <!-- Insights mode -->
        <template v-else>
          <div class="flex-1 min-h-0 overflow-hidden">
            <InsightsView />
          </div>
        </template>
      </template>

      <!-- Placeholder when no dataset is loaded -->
      <template v-else>
        <div class="flex-1 flex items-center justify-center">
          <div class="text-center text-muted">
            <p v-if="!connectionStore.isConnected">Connecting to server...</p>
            <p v-else-if="loadingTable">Loading dataset...</p>
            <p v-else>Select a dataset to begin</p>
          </div>
        </div>
      </template>
    </div>
  </UApp>
</template>

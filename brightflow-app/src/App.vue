<script setup lang="ts">
import { ref, onMounted } from 'vue';
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
import ConnectView from './components/connect/ConnectView.vue';
import DatasetPickerModal from './components/layout/DatasetPickerModal.vue';
import WelcomeLanding from './components/layout/WelcomeLanding.vue';

const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();

const showDatasetPicker = ref(false);
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
      uiStore.setShowConnect(false);
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

function handleOpenConnect(): void {
  uiStore.setShowConnect(true);
}
</script>

<template>
  <UApp>
    <!-- Dataset picker modal -->
    <DatasetPickerModal
      :open="showDatasetPicker && connectionStore.isConnected"
      :loading="loadingTable"
      @select="handleDatasetSelect"
      @close="showDatasetPicker = false"
    />

    <div class="h-screen flex flex-col bg-default">
      <!-- Header with change dataset action -->
      <AppHeader
        :current-dataset="currentDataset"
        @change-dataset="handleChangeDataset"
      />

      <!-- Connect mode - works without a dataset -->
      <template v-if="uiStore.showConnect">
        <div class="flex-1 min-h-0 overflow-hidden">
          <ConnectView />
        </div>
      </template>

      <!-- Explore / Insights - require a loaded dataset -->
      <template v-else-if="currentDataset">
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

      <!-- Welcome landing page when no dataset is loaded -->
      <template v-else>
        <div v-if="!connectionStore.isConnected" class="flex-1 flex items-center justify-center">
          <p class="text-muted">Connecting to server...</p>
        </div>
        <div v-else-if="loadingTable" class="flex-1 flex items-center justify-center">
          <p class="text-muted">Loading dataset...</p>
        </div>
        <WelcomeLanding
          v-else
          @load-dataset="handleChangeDataset"
          @open-connect="handleOpenConnect"
        />
      </template>
    </div>
  </UApp>
</template>

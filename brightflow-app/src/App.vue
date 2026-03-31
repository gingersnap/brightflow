<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';

import LoginPage from './components/auth/LoginPage.vue';
import ConnectView from './components/connect/ConnectView.vue';
import InsightsView from './components/insights/InsightsView.vue';
import AppHeader from './components/layout/AppHeader.vue';
import DatasetPickerModal from './components/layout/DatasetPickerModal.vue';
import WelcomeLanding from './components/layout/WelcomeLanding.vue';
import FilterBar from './components/query/FilterBar.vue';
import QueryBuilder from './components/query/QueryBuilder.vue';
import ResultsPanel from './components/results/ResultsPanel.vue';
import SystemView from './components/system/SystemView.vue';
import { type TableInfo, tableApi } from './services/api';
import { resetAllStores } from './stores';
import { useAuthStore } from './stores/auth';
import { useConnectionStore } from './stores/connection';
import { useDatasetStore } from './stores/dataset';
import { useSystemStore } from './stores/system';
import { useUiStore } from './stores/ui';

const authStore = useAuthStore();
const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();
const systemStore = useSystemStore();

const showDatasetPicker = ref(false);
const currentDataset = ref<string | null>(null);
const loadingTable = ref(false);

onMounted(() => {
  console.log('[App] Mounting, checking auth...');
  authStore.checkAuth();
});

// Connect/disconnect system WS when toggling system view
watch(
  () => uiStore.showSystem,
  (show) => {
    if (show) {
      systemStore.connect();
    } else {
      systemStore.disconnect();
    }
  },
);

// Connect WS when dataset is loaded, disconnect when cleared
watch(currentDataset, (newVal) => {
  if (newVal) {
    connectionStore.connect();
  } else {
    connectionStore.disconnect();
  }
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
      // Set dataset state from REST response (no WS needed for metadata)
      datasetStore.setFromLoadResponse(result);
      currentDataset.value = table.name;
      showDatasetPicker.value = false;
      uiStore.setShowConnect(false);
    }
  } catch (error) {
    console.error('[App] Failed to load table:', error);
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

async function handleLogout(): Promise<void> {
  connectionStore.disconnect();
  systemStore.disconnect();
  currentDataset.value = null;
  resetAllStores();
  await authStore.logout();
}
</script>

<template>
  <UApp>
    <!-- Auth loading state -->
    <div v-if="authStore.loading" class="flex h-screen items-center justify-center bg-default">
      <p class="text-muted">Loading...</p>
    </div>

    <!-- Login page -->
    <div v-else-if="!authStore.isAuthenticated" class="flex h-screen flex-col bg-default">
      <LoginPage />
    </div>

    <!-- Main app (authenticated) -->
    <template v-else>
      <!-- Dataset picker modal -->
      <DatasetPickerModal
        :open="showDatasetPicker"
        :loading="loadingTable"
        @select="handleDatasetSelect"
        @close="showDatasetPicker = false"
      />

      <div class="flex h-screen flex-col bg-default">
        <!-- Header with change dataset action -->
        <AppHeader
          :current-dataset="currentDataset"
          @change-dataset="handleChangeDataset"
          @logout="handleLogout"
        />

        <!-- System mode - works without a dataset -->
        <template v-if="uiStore.showSystem">
          <div class="relative min-h-0 flex-1 overflow-hidden">
            <SystemView />
          </div>
        </template>

        <!-- Connect mode - works without a dataset -->
        <template v-else-if="uiStore.showConnect">
          <div class="min-h-0 flex-1 overflow-hidden">
            <ConnectView />
          </div>
        </template>

        <!-- Explore / Insights - require a loaded dataset -->
        <template v-else-if="currentDataset">
          <!-- Explore mode -->
          <template v-if="uiStore.appMode === 'explore'">
            <FilterBar />
            <QueryBuilder />
            <div class="min-h-0 flex-1 overflow-hidden">
              <ResultsPanel />
            </div>
          </template>

          <!-- Insights mode -->
          <template v-else>
            <div class="min-h-0 flex-1 overflow-hidden">
              <InsightsView />
            </div>
          </template>
        </template>

        <!-- Welcome landing page when no dataset is loaded -->
        <template v-else>
          <div v-if="loadingTable" class="flex flex-1 items-center justify-center">
            <p class="text-muted">Loading dataset...</p>
          </div>
          <WelcomeLanding
            v-else
            @load-dataset="handleChangeDataset"
            @open-connect="handleOpenConnect"
          />
        </template>
      </div>
    </template>
  </UApp>
</template>

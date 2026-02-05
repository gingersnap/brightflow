<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { Database, ChevronDown, RefreshCw } from 'lucide-vue-next';
import { useDatasetStore } from '@/stores/dataset';
import { useConnectionStore } from '@/stores/connection';

const datasetStore = useDatasetStore();
const connectionStore = useConnectionStore();

// Fetch datasets when connected
onMounted(() => {
  if (connectionStore.isConnected) {
    datasetStore.fetchAvailableDatasets();
  }
});

// Re-fetch when connection changes
connectionStore.$subscribe(() => {
  if (connectionStore.isConnected && datasetStore.availableDatasets.length === 0) {
    datasetStore.fetchAvailableDatasets();
  }
});

// Items for dropdown
const datasetItems = computed(() =>
  datasetStore.availableDatasets.map((d) => ({
    label: d.name,
    value: d.id,
    rowCount: d.rowCount,
    suffix: d.rowCount ? `${d.rowCount.toLocaleString()} rows` : undefined,
  })),
);

// Current selection
const currentDatasetId = computed(() => datasetStore.id);

// Handle selection change
function onSelect(datasetId: string): void {
  datasetStore.switchDataset(datasetId);
}

// Refresh dataset list
function refresh(): void {
  datasetStore.fetchAvailableDatasets();
}

// Get display name for current dataset
const currentDisplayName = computed(() => {
  if (datasetStore.name) return datasetStore.name;
  const found = datasetStore.availableDatasets.find((d) => d.id === datasetStore.id);
  return found?.name ?? 'No dataset';
});
</script>

<template>
  <div class="flex items-center gap-1">
    <!-- Dataset dropdown -->
    <USelectMenu
      :model-value="currentDatasetId"
      :items="datasetItems"
      value-key="value"
      :disabled="!connectionStore.isConnected || datasetStore.loading"
      class="min-w-[180px]"
      @update:model-value="onSelect"
    >
      <template #leading>
        <Database class="w-4 h-4 text-muted" />
      </template>

      <template #default>
        <span class="truncate">{{ currentDisplayName }}</span>
        <span v-if="datasetStore.rowCount" class="ml-2 text-xs text-muted">
          ({{ datasetStore.rowCount.toLocaleString() }})
        </span>
      </template>

      <template #trailing>
        <ChevronDown class="w-4 h-4 text-muted" />
      </template>

      <!-- Custom option display -->
      <template #item="{ item }">
        <div class="flex items-center justify-between w-full gap-2">
          <span class="truncate">{{ item.label }}</span>
          <span v-if="item.suffix" class="text-xs text-muted shrink-0">
            {{ item.suffix }}
          </span>
        </div>
      </template>
    </USelectMenu>

    <!-- Refresh button -->
    <UButton
      variant="ghost"
      size="xs"
      square
      :disabled="!connectionStore.isConnected || datasetStore.loadingList"
      @click="refresh"
    >
      <RefreshCw
        class="w-3.5 h-3.5"
        :class="{ 'animate-spin': datasetStore.loadingList }"
      />
    </UButton>
  </div>
</template>

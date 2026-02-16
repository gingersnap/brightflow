<script setup lang="ts">
import { Upload, Settings, Database, ChevronDown } from 'lucide-vue-next';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';

defineProps<{
  currentDataset: string | null;
}>();

const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();

const emit = defineEmits<{
  upload: [];
  'change-dataset': [];
}>();
</script>

<template>
  <div class="flex h-14 items-center justify-between px-4 border-b border-default bg-default">
    <!-- Left: Logo and Current Dataset -->
    <div class="flex items-center gap-4">
      <h1 class="text-lg font-semibold text-highlighted">Brightflow</h1>

      <!-- Current dataset display with change button -->
      <button
        v-if="currentDataset"
        class="flex items-center gap-2 px-3 py-1.5 rounded-lg border border-default hover:border-primary-500/50 hover:bg-elevated transition-colors"
        @click="emit('change-dataset')"
      >
        <Database class="w-4 h-4 text-muted" />
        <span class="font-medium">{{ datasetStore.name ?? currentDataset }}</span>
        <span v-if="datasetStore.rowCount" class="text-xs text-muted">
          ({{ datasetStore.rowCount.toLocaleString() }} rows)
        </span>
        <ChevronDown class="w-4 h-4 text-muted" />
      </button>
    </div>

    <!-- Right: Actions and Status -->
    <div class="flex items-center gap-3">
      <!-- Upload Button -->
      <UButton
        variant="ghost"
        size="sm"
        @click="emit('upload')"
      >
        <Upload class="w-4 h-4 mr-1.5" />
        Upload CSV
      </UButton>

      <!-- Settings (placeholder) -->
      <UButton
        variant="ghost"
        size="sm"
        square
      >
        <Settings class="w-4 h-4" />
      </UButton>

      <!-- Connection Status -->
      <div class="flex items-center gap-2 pl-3 border-l border-default">
        <span
          class="h-2 w-2 rounded-full"
          :class="{
            'bg-green-500': connectionStore.isConnected,
            'bg-yellow-500 animate-pulse': connectionStore.isConnecting,
            'bg-red-500': connectionStore.isDisconnected
          }"
        />
        <span class="text-xs text-muted">
          {{ connectionStore.statusText }}
        </span>
      </div>
    </div>
  </div>
</template>

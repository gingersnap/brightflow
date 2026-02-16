<script setup lang="ts">
import { Upload, Settings, Database, ChevronDown, Search, Sparkles } from 'lucide-vue-next';
import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { useUiStore, type AppMode } from '@/stores/ui';

defineProps<{
  currentDataset: string | null;
}>();

const connectionStore = useConnectionStore();
const datasetStore = useDatasetStore();
const uiStore = useUiStore();

const emit = defineEmits<{
  upload: [];
  'change-dataset': [];
}>();

function setMode(mode: AppMode): void {
  uiStore.setAppMode(mode);
}
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

      <!-- Mode switcher: Explore / Insights -->
      <div v-if="currentDataset" class="flex items-center gap-1 bg-elevated rounded-lg p-0.5 ml-2">
        <button
          class="flex items-center gap-1.5 px-3 py-1 text-xs font-medium rounded-md transition-colors"
          :class="
            uiStore.appMode === 'explore'
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="setMode('explore')"
        >
          <Search class="w-3.5 h-3.5" />
          Explore
        </button>
        <button
          class="flex items-center gap-1.5 px-3 py-1 text-xs font-medium rounded-md transition-colors"
          :class="
            uiStore.appMode === 'insights'
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="setMode('insights')"
        >
          <Sparkles class="w-3.5 h-3.5" />
          Insights
        </button>
      </div>
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

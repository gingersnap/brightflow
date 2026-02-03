<script setup>
import { Upload, Settings, Database } from 'lucide-vue-next'
import { useConnectionStore } from '@/stores/connection'
import { useDatasetStore } from '@/stores/dataset'

const connectionStore = useConnectionStore()
const datasetStore = useDatasetStore()

const emit = defineEmits(['upload'])
</script>

<template>
  <div class="flex h-14 items-center justify-between px-4 border-b border-default bg-default">
    <!-- Left: Logo and Dataset -->
    <div class="flex items-center gap-4">
      <h1 class="text-lg font-semibold text-highlighted">Brightflow</h1>

      <div class="flex items-center gap-2 px-3 py-1.5 rounded-md bg-muted/50">
        <Database class="w-4 h-4 text-muted" />
        <span class="text-sm font-medium">
          {{ datasetStore.name || 'No dataset' }}
        </span>
        <span v-if="datasetStore.rowCount" class="text-xs text-muted">
          ({{ datasetStore.rowCount.toLocaleString() }} rows)
        </span>
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

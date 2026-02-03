<script setup>
import { onMounted } from 'vue'
import { useConnectionStore } from './stores/connection'
import { useDatasetStore } from './stores/dataset'

import AppHeader from './components/layout/AppHeader.vue'
import FilterBar from './components/query/FilterBar.vue'
import QueryBuilder from './components/query/QueryBuilder.vue'
import ResultsPanel from './components/results/ResultsPanel.vue'

const connectionStore = useConnectionStore()
const datasetStore = useDatasetStore()

onMounted(() => {
  console.log('[App] Mounting, connecting to WebSocket...')
  connectionStore.connect()
})

function handleUpload() {
  // TODO: Implement upload modal
  console.log('Upload clicked')
}
</script>

<template>
  <UApp>
    <div class="h-screen flex flex-col bg-default">
      <!-- Header -->
      <AppHeader @upload="handleUpload" />

      <!-- Filter Bar -->
      <FilterBar />

      <!-- Query Builder -->
      <QueryBuilder />

      <!-- Results Panel (fills remaining space) -->
      <div class="flex-1 min-h-0 overflow-hidden">
        <ResultsPanel />
      </div>
    </div>
  </UApp>
</template>

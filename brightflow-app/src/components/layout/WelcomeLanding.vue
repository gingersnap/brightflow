<script setup lang="ts">
import { Cable, Database } from 'lucide-vue-next';

defineEmits<{
  'load-dataset': [];
  'open-connect': [];
}>();
</script>

<template>
  <div class="flex-1 flex items-center justify-center">
    <div class="text-center max-w-md">
      <h2 class="text-2xl font-semibold text-highlighted mb-2">Welcome to Brightflow</h2>
      <p class="text-muted mb-8">
        Load a dataset to start exploring your data, or set up connectors to sync from external
        sources.
      </p>
      <div class="flex items-center justify-center gap-4">
        <UButton size="lg" @click="$emit('load-dataset')">
          <Database class="w-4 h-4" />
          Load a Dataset
        </UButton>
        <UButton size="lg" variant="outline" @click="$emit('open-connect')">
          <Cable class="w-4 h-4" />
          Set Up Connectors
        </UButton>
      </div>
    </div>
  </div>
</template>

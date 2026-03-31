<script setup lang="ts">
import { Cable, Database } from 'lucide-vue-next';

defineEmits<{
  'load-dataset': [];
  'open-connect': [];
}>();
</script>

<template>
  <div class="flex flex-1 items-center justify-center">
    <div class="max-w-md text-center">
      <h2 class="mb-2 text-2xl font-semibold text-highlighted">Welcome to Brightflow</h2>
      <p class="mb-8 text-muted">
        Load a dataset to start exploring your data, or set up connectors to sync from external
        sources.
      </p>
      <div class="flex items-center justify-center gap-4">
        <UButton size="lg" @click="$emit('load-dataset')">
          <Database class="h-4 w-4" />
          Load a Dataset
        </UButton>
        <UButton size="lg" variant="outline" @click="$emit('open-connect')">
          <Cable class="h-4 w-4" />
          Set Up Connectors
        </UButton>
      </div>
    </div>
  </div>
</template>

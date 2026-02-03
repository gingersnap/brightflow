<script setup>
import QueryBuilder from '@/components/query-builder/QueryBuilder.vue'
import ResultsPanel from '@/components/results/ResultsPanel.vue'
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Query Builder Section -->
    <div class="border-b border-default overflow-y-auto max-h-[50%] min-h-[200px]">
      <QueryBuilder />
    </div>

    <!-- Results Section -->
    <div class="flex-1 overflow-hidden min-h-0">
      <ResultsPanel />
    </div>
  </div>
</template>

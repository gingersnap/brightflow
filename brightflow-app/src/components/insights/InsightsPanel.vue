<script setup lang="ts">
import { computed } from 'vue';
import { FileSearch } from 'lucide-vue-next';
import { useInsightsStore } from '@/stores/insights';
import type { AnalysisNode } from '@/services/api';
import InsightCard from './InsightCard.vue';

const insightsStore = useInsightsStore();

// Get root nodes sorted by significance
const rootNodes = computed(() => {
  if (!insightsStore.tree) return [];
  return insightsStore.tree.roots
    .map((rootId) => insightsStore.tree?.nodes.find((n) => n.id['0'] === rootId['0']))
    .filter((n): n is AnalysisNode => n !== undefined)
    .sort((a, b) => b.significance - a.significance);
});
</script>

<template>
  <div class="h-full overflow-y-auto">
    <!-- Loading state -->
    <div v-if="insightsStore.loading" class="flex items-center justify-center h-64">
      <div class="text-center">
        <div class="inline-block w-6 h-6 border-2 border-primary-500 border-t-transparent rounded-full animate-spin mb-3" />
        <p class="text-sm text-muted">Running analysis...</p>
      </div>
    </div>

    <!-- Error state -->
    <div v-else-if="insightsStore.error" class="p-6">
      <div class="border border-red-500/20 bg-red-500/5 rounded-lg p-4">
        <p class="text-sm text-red-500">{{ insightsStore.error }}</p>
      </div>
    </div>

    <!-- Empty state -->
    <div v-else-if="!insightsStore.tree" class="flex items-center justify-center h-64">
      <div class="text-center">
        <FileSearch class="w-10 h-10 text-muted mx-auto mb-3" />
        <p class="text-sm text-muted">Select a report type and click Run to start analysis</p>
      </div>
    </div>

    <!-- No findings -->
    <div v-else-if="rootNodes.length === 0" class="flex items-center justify-center h-64">
      <div class="text-center">
        <p class="text-sm text-muted">No significant findings detected</p>
        <p class="text-xs text-muted mt-1">Try a different report type or cadence</p>
      </div>
    </div>

    <!-- Results -->
    <div v-else class="p-4 space-y-3">
      <!-- Summary bar -->
      <div class="flex items-center justify-between text-xs text-muted pb-2 border-b border-default">
        <span>{{ rootNodes.length }} finding{{ rootNodes.length === 1 ? '' : 's' }}</span>
        <span v-if="insightsStore.executionTimeMs !== null">
          {{ insightsStore.executionTimeMs.toFixed(0) }}ms
        </span>
      </div>

      <!-- Finding cards -->
      <InsightCard
        v-for="node in rootNodes"
        :key="node.id['0']"
        :node="node"
        :tree="insightsStore.tree"
      />
    </div>
  </div>
</template>

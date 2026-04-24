<script setup lang="ts">
import { FileSearch } from 'lucide-vue-next';
import { computed } from 'vue';

import type { AnalysisNode } from '@/services/api';
import { useInsightsStore } from '@/stores/insights';

import InsightCard from './InsightCard.vue';

const insightsStore = useInsightsStore();

// Get root nodes sorted by significance
const rootNodes = computed(() => {
  if (!insightsStore.tree) {
    return [];
  }
  return insightsStore.tree.roots
    .map((rootId) => insightsStore.tree?.nodes.find((n) => n.id === rootId))
    .filter((n): n is AnalysisNode => n !== undefined)
    .toSorted((a, b) => b.significance - a.significance);
});
</script>

<template>
  <div class="h-full overflow-y-auto">
    <!-- Loading state -->
    <div v-if="insightsStore.loading" class="flex h-64 items-center justify-center">
      <div class="text-center">
        <div
          class="mb-3 inline-block h-6 w-6 animate-spin rounded-full border-2 border-primary-500 border-t-transparent"
        />
        <p class="text-sm text-muted">Running analysis...</p>
      </div>
    </div>

    <!-- Error state -->
    <div v-else-if="insightsStore.error" class="p-6">
      <div class="rounded-lg border border-red-500/20 bg-red-500/5 p-4">
        <p class="text-sm text-red-500">{{ insightsStore.error }}</p>
      </div>
    </div>

    <!-- Empty state -->
    <div v-else-if="!insightsStore.tree" class="flex h-64 items-center justify-center">
      <div class="text-center">
        <FileSearch class="mx-auto mb-3 h-10 w-10 text-muted" />
        <p class="text-sm text-muted">Select a report type and click Run to start analysis</p>
      </div>
    </div>

    <!-- No findings -->
    <div v-else-if="rootNodes.length === 0" class="flex h-64 items-center justify-center">
      <div class="text-center">
        <p class="text-sm text-muted">No significant findings detected</p>
        <p class="mt-1 text-sm text-muted">Try a different report type or cadence</p>
      </div>
    </div>

    <!-- Results -->
    <div v-else class="space-y-3 p-4">
      <!-- Summary bar -->
      <div
        class="flex items-center justify-between border-b border-default pb-2 text-sm text-muted"
      >
        <span>
          {{ insightsStore.firstLevelCount + insightsStore.deeperCount }} analyses
          <template v-if="insightsStore.deeperCount > 0">
            ({{ insightsStore.firstLevelCount }} first-level,
            {{ insightsStore.deeperCount }} deeper)
          </template>
          &rarr; {{ rootNodes.length }} finding{{ rootNodes.length === 1 ? '' : 's' }}
        </span>
        <span v-if="insightsStore.executionTimeMs !== null">
          {{ insightsStore.executionTimeMs.toFixed(0) }}ms
        </span>
      </div>

      <!-- Finding cards -->
      <InsightCard
        v-for="node in rootNodes"
        :key="node.id"
        :node="node"
        :tree="insightsStore.tree"
      />
    </div>
  </div>
</template>

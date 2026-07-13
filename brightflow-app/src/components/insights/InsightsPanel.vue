<script setup lang="ts">
import { FileSearch, Filter as FilterIcon } from '@lucide/vue';
import { computed, ref } from 'vue';

import { useInsightsStore } from '@/stores/insights';

import InsightCard from './InsightCard.vue';

const insightsStore = useInsightsStore();

const TOP_N = 5;

const allRoots = computed(() => insightsStore.visibleRoots);

const visibleSlice = computed(() =>
  insightsStore.showAll ? allRoots.value : allRoots.value.slice(0, TOP_N),
);

const hiddenCount = computed(() => Math.max(0, allRoots.value.length - TOP_N));

const filtersOpen = ref(false);

function toggleType(t: string): void {
  const next = new Set(insightsStore.filters.types);
  if (next.has(t)) {
    next.delete(t);
  } else {
    next.add(t);
  }
  insightsStore.filters.types = next;
}

const directionOptions: { label: string; value: 'up' | 'down' | 'both' }[] = [
  { label: 'All', value: 'both' },
  { label: 'Up', value: 'up' },
  { label: 'Down', value: 'down' },
];
</script>

<template>
  <div class="h-full overflow-y-auto">
    <!-- Loading -->
    <div v-if="insightsStore.loading" class="flex h-64 items-center justify-center">
      <div class="text-center">
        <UIcon
          name="i-lucide-loader-circle"
          class="mb-3 inline-block size-6 animate-spin text-primary"
        />
        <p class="text-sm text-muted">Running analysis...</p>
      </div>
    </div>

    <!-- Error -->
    <div v-else-if="insightsStore.error" class="p-6">
      <div class="rounded-lg border border-red-500/20 bg-red-500/5 p-4">
        <p class="text-sm text-red-500">{{ insightsStore.error }}</p>
      </div>
    </div>

    <!-- Empty -->
    <div v-else-if="!insightsStore.tree" class="flex h-64 items-center justify-center">
      <div class="text-center">
        <FileSearch class="mx-auto mb-3 h-10 w-10 text-muted" />
        <p class="text-sm text-muted">Select a report type and click Run to start analysis</p>
      </div>
    </div>

    <!-- No findings -->
    <div
      v-else-if="allRoots.length === 0 && insightsStore.totalCandidates > 0"
      class="flex h-64 items-center justify-center"
    >
      <div class="text-center">
        <p class="text-sm text-muted">
          No findings match the current filters
          <span v-if="insightsStore.totalCandidates > 0">
            ({{ insightsStore.totalCandidates.toLocaleString() }} analyses ran)
          </span>
        </p>
        <p class="mt-1 text-sm text-muted">Adjust filters or try a different report type</p>
      </div>
    </div>

    <!-- Results -->
    <div v-else class="space-y-3 p-4">
      <!-- Scale + filter bar -->
      <div
        class="flex items-center justify-between border-b border-default pb-2 text-sm text-muted"
      >
        <span>
          Showing
          <span class="font-medium text-highlighted">{{ visibleSlice.length }}</span>
          of
          <span class="font-medium text-highlighted">{{ allRoots.length }}</span>
          findings
          <span v-if="insightsStore.totalCandidates > 0" class="text-muted">
            · {{ insightsStore.totalCandidates.toLocaleString() }} analyses ran
          </span>
        </span>
        <div class="flex items-center gap-3">
          <span v-if="insightsStore.executionTimeMs !== null" class="font-mono-data">
            {{ insightsStore.executionTimeMs.toFixed(0) }}ms
          </span>
          <button
            class="flex cursor-pointer items-center gap-1 rounded-md px-2 py-1 hover:bg-elevated"
            :class="filtersOpen ? 'text-highlighted' : ''"
            @click="filtersOpen = !filtersOpen"
          >
            <FilterIcon class="h-3.5 w-3.5" />
            Filters
          </button>
        </div>
      </div>

      <!-- Filter chrome -->
      <div
        v-if="filtersOpen"
        class="flex flex-col gap-3 rounded-lg border border-default bg-elevated/30 p-3"
      >
        <!-- Type chips -->
        <div
          v-if="insightsStore.availableTypes.length > 0"
          class="flex flex-wrap items-center gap-1"
        >
          <span class="mr-2 text-xs tracking-wider text-muted uppercase">Type</span>
          <button
            v-for="t in insightsStore.availableTypes"
            :key="t"
            class="cursor-pointer rounded-full border border-default px-2.5 py-0.5 text-sm transition-colors"
            :class="
              insightsStore.filters.types.has(t)
                ? 'border-primary-500/50 bg-primary-500/10 text-highlighted'
                : 'text-muted hover:text-highlighted'
            "
            @click="toggleType(t)"
          >
            {{ t }}
          </button>
        </div>

        <!-- Direction toggle -->
        <div class="flex items-center gap-2">
          <span class="mr-2 text-xs tracking-wider text-muted uppercase">Direction</span>
          <div class="flex items-center gap-1 rounded-md bg-default p-0.5">
            <button
              v-for="d in directionOptions"
              :key="d.value"
              class="cursor-pointer rounded px-2.5 py-0.5 text-sm transition-colors"
              :class="
                insightsStore.filters.direction === d.value
                  ? 'bg-elevated text-highlighted'
                  : 'text-muted hover:text-highlighted'
              "
              @click="insightsStore.filters.direction = d.value"
            >
              {{ d.label }}
            </button>
          </div>
        </div>

        <!-- Measure dropdown -->
        <div v-if="insightsStore.availableMeasures.length > 0" class="flex items-center gap-2">
          <span class="mr-2 text-xs tracking-wider text-muted uppercase">Measure</span>
          <select
            v-model="insightsStore.filters.measure"
            class="rounded-md border border-default bg-default px-2 py-1 text-sm text-highlighted"
          >
            <option :value="null">All measures</option>
            <option v-for="m in insightsStore.availableMeasures" :key="m" :value="m">
              {{ m }}
            </option>
          </select>
        </div>

        <!-- Min score -->
        <div class="flex items-center gap-2">
          <span class="mr-2 text-xs tracking-wider text-muted uppercase">Min score</span>
          <input
            v-model.number="insightsStore.filters.minScore"
            type="range"
            min="0"
            max="2"
            step="0.05"
            class="flex-1"
          />
          <span class="w-10 font-mono-data text-sm text-muted">
            {{ insightsStore.filters.minScore.toFixed(2) }}
          </span>
        </div>
      </div>

      <!-- Findings -->
      <InsightCard
        v-for="node in visibleSlice"
        :key="node.id"
        :node="node"
        :tree="insightsStore.tree"
      />

      <!-- See more -->
      <button
        v-if="!insightsStore.showAll && hiddenCount > 0"
        class="w-full cursor-pointer rounded-lg border border-dashed border-default py-3 text-sm text-muted transition-colors hover:border-primary-500/50 hover:text-highlighted"
        @click="insightsStore.showAll = true"
      >
        Show {{ hiddenCount }} more finding{{ hiddenCount === 1 ? '' : 's' }}
      </button>
      <button
        v-else-if="insightsStore.showAll && hiddenCount > 0"
        class="w-full cursor-pointer rounded-lg border border-dashed border-default py-3 text-sm text-muted transition-colors hover:border-primary-500/50 hover:text-highlighted"
        @click="insightsStore.showAll = false"
      >
        Show top {{ TOP_N }} only
      </button>
    </div>
  </div>
</template>

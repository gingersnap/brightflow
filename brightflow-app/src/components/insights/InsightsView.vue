<script setup lang="ts">
import { Play } from 'lucide-vue-next';

import { type Cadence, type ReportType, useInsightsStore } from '@/stores/insights';

import InsightsPanel from './InsightsPanel.vue';

const insightsStore = useInsightsStore();

const reportTypes: { value: ReportType; label: string }[] = [
  { label: 'Review', value: 'review' },
  { label: 'Trends', value: 'trends' },
];

const cadences: { value: Cadence; label: string }[] = [
  { label: 'Daily', value: 'daily' },
  { label: 'Weekly', value: 'weekly' },
  { label: 'Monthly', value: 'monthly' },
];

function runAnalysis(): void {
  if (insightsStore.reportType === 'review') {
    insightsStore.runReview(insightsStore.cadence);
  } else {
    insightsStore.runTrends();
  }
}

function selectReportType(type: ReportType): void {
  insightsStore.reportType = type;
}

function selectCadence(c: Cadence): void {
  insightsStore.cadence = c;
}
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Toolbar -->
    <div class="flex items-center gap-3 px-4 py-2.5 border-b border-default bg-default">
      <!-- Report type selector -->
      <div class="flex items-center gap-1 bg-elevated rounded-lg p-0.5">
        <button
          v-for="rt in reportTypes"
          :key="rt.value"
          class="px-3 py-1 text-xs font-medium rounded-md transition-colors"
          :class="
            insightsStore.reportType === rt.value
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="selectReportType(rt.value)"
        >
          {{ rt.label }}
        </button>
      </div>

      <!-- Cadence selector (only for review) -->
      <div
        v-if="insightsStore.reportType === 'review'"
        class="flex items-center gap-1 bg-elevated rounded-lg p-0.5"
      >
        <button
          v-for="c in cadences"
          :key="c.value"
          class="px-3 py-1 text-xs font-medium rounded-md transition-colors"
          :class="
            insightsStore.cadence === c.value
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="selectCadence(c.value)"
        >
          {{ c.label }}
        </button>
      </div>

      <!-- Spacer -->
      <div class="flex-1" />

      <!-- Run button -->
      <UButton size="sm" :loading="insightsStore.loading" @click="runAnalysis">
        <Play class="w-3.5 h-3.5 mr-1.5" />
        Run Analysis
      </UButton>
    </div>

    <!-- Results -->
    <div class="flex-1 min-h-0">
      <InsightsPanel />
    </div>
  </div>
</template>

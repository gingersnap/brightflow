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
  <div class="flex h-full flex-col">
    <!-- Toolbar -->
    <div class="flex items-center gap-3 border-b border-default bg-default px-4 py-2.5">
      <!-- Report type selector -->
      <div class="flex items-center gap-1 rounded-lg bg-elevated p-0.5">
        <button
          v-for="rt in reportTypes"
          :key="rt.value"
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors"
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
        class="flex items-center gap-1 rounded-lg bg-elevated p-0.5"
      >
        <button
          v-for="c in cadences"
          :key="c.value"
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors"
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
        <Play class="mr-1.5 h-3.5 w-3.5" />
        Run Analysis
      </UButton>
    </div>

    <!-- Results -->
    <div class="min-h-0 flex-1">
      <InsightsPanel />
    </div>
  </div>
</template>

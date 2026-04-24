<script setup lang="ts">
import { ArrowLeft, Play, Table2 } from 'lucide-vue-next';
import { computed, watch } from 'vue';
import { useRouter } from 'vue-router';

import { type Cadence, type ReportType, useInsightsStore } from '@/stores/insights';
import { useSourceStore } from '@/stores/source';

import InsightsPanel from './InsightsPanel.vue';

const props = defineProps<{
  sourceId: string;
  table?: string;
}>();

const router = useRouter();
const sourceStore = useSourceStore();
const insightsStore = useInsightsStore();

const tables = computed(() => sourceStore.getSourceById(props.sourceId)?.tables ?? []);

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

function handleSelectTable(name: string): void {
  router.push({
    name: 'insights-table',
    params: { sourceId: props.sourceId, table: name },
  });
}

function handleBackToTables(): void {
  insightsStore.reset();
  router.push({ name: 'source-tool', params: { sourceId: props.sourceId, tool: 'insights' } });
}

// Watch table prop — select in store when it changes
watch(
  () => [props.sourceId, props.table] as const,
  ([sourceId, name]) => {
    if (name) {
      insightsStore.selectTable(sourceId, name);
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Table picker phase -->
    <template v-if="!table">
      <div class="p-6">
        <h2 class="mb-4 text-lg font-semibold text-highlighted">Select a table to analyze</h2>
        <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <button
            v-for="t in tables"
            :key="t.name"
            class="flex cursor-pointer items-center gap-3 rounded-lg border border-default bg-elevated p-4 text-left transition-all hover:border-primary-500/50 hover:shadow-sm"
            @click="handleSelectTable(t.name)"
          >
            <Table2 class="h-5 w-5 text-muted" />
            <div>
              <p class="text-sm font-medium text-highlighted">{{ t.name }}</p>
              <p v-if="t.numRows != null" class="text-sm text-muted">
                {{ t.numRows.toLocaleString() }} rows
              </p>
            </div>
          </button>
        </div>
      </div>
    </template>

    <!-- Analysis phase -->
    <template v-else>
      <!-- Toolbar -->
      <div class="flex items-center gap-3 border-b border-default bg-default px-4 py-2.5">
        <button
          class="flex cursor-pointer items-center gap-1 text-sm text-muted transition-colors hover:text-highlighted"
          @click="handleBackToTables"
        >
          <ArrowLeft class="h-3 w-3" />
          Back to tables
        </button>

        <span class="text-sm font-medium text-highlighted">{{ table }}</span>

        <div class="mx-1 h-4 w-px bg-default" />

        <!-- Report type selector -->
        <div class="flex items-center gap-1 rounded-lg bg-elevated p-0.5">
          <button
            v-for="rt in reportTypes"
            :key="rt.value"
            class="rounded-md px-3 py-1 text-sm font-medium transition-colors"
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
            class="rounded-md px-3 py-1 text-sm font-medium transition-colors"
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
        <UButton size="md" :loading="insightsStore.loading" @click="runAnalysis">
          <Play class="mr-1.5 h-3.5 w-3.5" />
          Run Analysis
        </UButton>
      </div>

      <!-- Results -->
      <div class="min-h-0 flex-1">
        <InsightsPanel />
      </div>
    </template>
  </div>
</template>

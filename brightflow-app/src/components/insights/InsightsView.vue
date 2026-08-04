<script setup lang="ts">
/**
 * Route-level shell for the Insights tool: table picker, report-type and
 * cadence toolbar, and the Run trigger, with agent curation, narration,
 * history, and activity panels arranged around the results. Mounts the
 * store's realtime curation sync on open, and marks a table's findings
 * seen both on open and after each run.
 */

import { onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import ActivityFeed from '@/components/actions/ActivityFeed.vue';
import AgentActions from '@/components/actions/AgentActions.vue';
import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import { useInsightsRuns } from '@/composables/useInsightsRuns';
import { insightHistoryApi } from '@/services/api';
import { type Cadence, type ReportType, useInsightsStore } from '@/stores/insights';
import { useInsightsActivityStore } from '@/stores/insightsActivity';
import type { SourceTable } from '@/types';

import InsightHistoryPanel from './InsightHistoryPanel.vue';
import InsightsPanel from './InsightsPanel.vue';
import NarrationPanel from './NarrationPanel.vue';

const props = defineProps<{
  sourceId: string;
  table?: string | undefined;
}>();

const router = useRouter();
const insightsStore = useInsightsStore();
const insightsRuns = useInsightsRuns();
const insightsActivity = useInsightsActivityStore();
const activityOpen = ref(false);
const historyOpen = ref(false);
const historyResetNote = ref<string | null>(null);

onMounted(() => {
  // Live curation: dismiss/pin/suppress events (own tab, agent runs, Activity
  // Undo) move cards without a re-run.
  insightsStore.initCurationSync();
  insightsActivity.initRealtime();
});

async function resetHistory(): Promise<void> {
  if (insightsStore.selectedTable == null) {
    return;
  }
  const result = await insightHistoryApi.reset(props.sourceId, insightsStore.selectedTable);
  historyResetNote.value =
    result == null ? 'Reset failed' : `Cleared ${result.deleted} remembered insights`;
}

const reportTypes: { value: ReportType; label: string; hint: string }[] = [
  {
    hint: 'What changed in the latest period vs the one before, and which segments drove it',
    label: 'Review',
    value: 'review',
  },
  {
    hint: 'What is changing over time — trends, seasonality, shifts, and forecast misses',
    label: 'Trends',
    value: 'trends',
  },
  {
    hint: 'What makes up each KPI and which segments drove its latest change',
    label: 'Drivers',
    value: 'drivers',
  },
];

const cadences: { value: Cadence; label: string; hint: string }[] = [
  { hint: 'Compare today with yesterday', label: 'Daily', value: 'daily' },
  { hint: 'Compare this week with last week', label: 'Weekly', value: 'weekly' },
  { hint: 'Compare this month with last month', label: 'Monthly', value: 'monthly' },
];

function runAnalysis(): void {
  const markSeen = (): void => {
    if (props.table) {
      insightsActivity.markSeen(props.sourceId, props.table);
    }
  };
  if (insightsStore.reportType === 'review') {
    void insightsRuns.runReview(insightsStore.cadence).then(markSeen);
  } else if (insightsStore.reportType === 'drivers') {
    void insightsRuns.runDrivers().then(markSeen);
  } else {
    void insightsRuns.runTrends().then(markSeen);
  }
}

function selectReportType(type: ReportType): void {
  insightsStore.reportType = type;
}

function selectCadence(c: Cadence): void {
  insightsStore.cadence = c;
}

function handleSelectTable(table: SourceTable): void {
  router.push({
    name: 'insights-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

function handleAutoSelectTable(table: SourceTable): void {
  router.replace({
    name: 'insights-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

// Watch table prop — select in store when it changes, reset on the bare route
watch(
  () => [props.sourceId, props.table] as const,
  ([sourceId, name]) => {
    if (name) {
      insightsStore.selectTable(sourceId, name);
      // Opening the table counts as seeing its latest findings.
      insightsActivity.markSeen(sourceId, name);
    } else {
      insightsStore.reset();
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full flex-col">
    <TableSectionPane
      :source-id="sourceId"
      :selected-table="table"
      @select-table="handleSelectTable"
      @auto-select-table="handleAutoSelectTable"
    />

    <div :inert="!table" :class="{ 'opacity-50': !table }">
      <!-- Toolbar -->
      <div class="flex items-center gap-3 border-b border-default bg-default px-4 py-2.5">
        <!-- Report type selector -->
        <div class="flex items-center gap-1 rounded-lg bg-elevated p-0.5">
          <UTooltip v-for="rt in reportTypes" :key="rt.value" :text="rt.hint">
            <button
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
          </UTooltip>
        </div>

        <!-- Cadence selector (only for review) -->
        <div
          v-if="insightsStore.reportType === 'review'"
          class="flex items-center gap-1 rounded-lg bg-elevated p-0.5"
        >
          <UTooltip v-for="c in cadences" :key="c.value" :text="c.hint">
            <button
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
          </UTooltip>
        </div>

        <!-- Spacer -->
        <div class="flex-1" />

        <!-- Run button -->
        <UButton size="md" :loading="insightsStore.loading" @click="runAnalysis">
          <UIcon name="i-lucide-play" class="mr-1.5 h-3.5 w-3.5" />
          Run Analysis
        </UButton>
      </div>
    </div>

    <!-- Agent curation (visible only with an LLM provider) -->
    <div v-if="insightsStore.selectedTable" class="px-4 pt-2">
      <div class="flex flex-wrap items-center gap-2">
        <AgentActions
          :source-id="sourceId"
          :table="insightsStore.selectedTable"
          :kinds="[
            { kind: 'triage_insights', label: 'Triage', icon: 'i-lucide-list-checks' },
            { kind: 'narrate_insights', label: 'Summarize', icon: 'i-lucide-scroll-text' },
          ]"
        />
        <UButton
          size="md"
          color="neutral"
          variant="ghost"
          icon="i-lucide-brain"
          title="Forget which insights were already shown — novelty scores reset to fresh"
          @click="resetHistory"
        >
          Reset insight memory
        </UButton>
        <UButton
          size="md"
          color="neutral"
          variant="ghost"
          icon="i-lucide-history"
          @click="historyOpen = true"
        >
          History
        </UButton>
        <span v-if="historyResetNote" class="text-sm text-muted">{{ historyResetNote }}</span>
      </div>
    </div>

    <!-- Agent narration (newest completed summarize/triage run) -->
    <div v-if="insightsStore.selectedTable" class="px-4 pt-2">
      <NarrationPanel :source-id="sourceId" :table="insightsStore.selectedTable" />
    </div>

    <!-- Results -->
    <div class="relative min-h-0 flex-1">
      <div class="h-full" :inert="!table" :class="{ 'opacity-50': !table }">
        <InsightsPanel />
      </div>
      <div v-if="!table" class="absolute inset-0 flex items-center justify-center bg-default/60">
        <p class="text-sm text-muted">Choose a table above to run an analysis</p>
      </div>
    </div>

    <UButton
      size="md"
      color="neutral"
      variant="soft"
      icon="i-lucide-history"
      class="fixed right-4 bottom-4 z-10 shadow-lg"
      @click="activityOpen = true"
    >
      Activity
    </UButton>
    <USlideover v-model:open="activityOpen" title="Activity">
      <template #body>
        <ActivityFeed />
      </template>
    </USlideover>
    <USlideover v-model:open="historyOpen" title="Insight history">
      <template #body>
        <InsightHistoryPanel
          v-if="insightsStore.selectedTable"
          :key="`${sourceId}:${insightsStore.selectedTable}:${String(historyOpen)}`"
          :source-id="sourceId"
          :table="insightsStore.selectedTable"
        />
      </template>
    </USlideover>
  </div>
</template>

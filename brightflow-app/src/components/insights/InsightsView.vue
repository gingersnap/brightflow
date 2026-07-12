<script setup lang="ts">
import { Play } from 'lucide-vue-next';
import { ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import ActivityFeed from '@/components/actions/ActivityFeed.vue';
import AgentActions from '@/components/actions/AgentActions.vue';
import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import { insightHistoryApi } from '@/services/api';
import { type Cadence, type ReportType, useInsightsStore } from '@/stores/insights';
import type { SourceTable } from '@/types';

import InsightsPanel from './InsightsPanel.vue';

const props = defineProps<{
  sourceId: string;
  table?: string;
}>();

const router = useRouter();
const insightsStore = useInsightsStore();
const activityOpen = ref(false);
const historyResetNote = ref<string | null>(null);

async function resetHistory(): Promise<void> {
  if (insightsStore.selectedTable == null) {
    return;
  }
  const result = await insightHistoryApi.reset(props.sourceId, insightsStore.selectedTable);
  historyResetNote.value =
    result == null ? 'Reset failed' : `Cleared ${result.deleted} remembered insights`;
}

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
        <span v-if="historyResetNote" class="text-sm text-muted">{{ historyResetNote }}</span>
      </div>
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
  </div>
</template>

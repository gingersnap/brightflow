<script setup lang="ts">
import { onMounted, ref } from 'vue';

import { insightHistoryApi, insightRunsApi } from '@/services/api';
import type { InsightHistoryRow } from '@/types';
import type { InsightRunResponse } from '@/types/generated';

/**
 * Run + story history for one table's insights.
 *
 * Runs = every computation (manual and post-sync auto-runs). Stories = the
 * novelty memory: which findings have been shown, how often, and when.
 */
const props = defineProps<{
  sourceId: string;
  table: string;
}>();

const runs = ref<InsightRunResponse[]>([]);
const stories = ref<InsightHistoryRow[]>([]);
const loading = ref(true);

onMounted(async () => {
  const [runsResult, storiesResult] = await Promise.all([
    insightRunsApi.list(props.sourceId, props.table, 25),
    insightHistoryApi.list(props.sourceId, props.table),
  ]);
  runs.value = runsResult ?? [];
  stories.value = (storiesResult ?? []).toSorted((a, b) => b.last_shown_at - a.last_shown_at);
  loading.value = false;
});

function formatEpoch(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

function triggerLabel(run: InsightRunResponse): string {
  return run.triggeredBy === 'post_sync' ? 'auto (after sync)' : 'manual';
}

function reportLabel(reportType: string): string {
  if (reportType.startsWith('review')) {
    return 'Review';
  }
  return reportType === 'drivers' ? 'Drivers' : 'Trends';
}
</script>

<template>
  <div class="space-y-6">
    <div v-if="loading" class="flex h-32 items-center justify-center">
      <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin text-muted" />
    </div>

    <template v-else>
      <!-- Runs -->
      <section>
        <h3 class="mb-2 text-xs tracking-wider text-muted uppercase">Runs</h3>
        <p v-if="runs.length === 0" class="text-sm text-muted">
          No analyses have run for this table yet
        </p>
        <ul v-else class="space-y-2">
          <li
            v-for="run in runs"
            :key="run.id"
            class="rounded-lg border border-default bg-elevated/30 p-3"
          >
            <div class="flex items-center justify-between gap-2 text-sm">
              <span class="font-medium text-highlighted">
                {{ reportLabel(run.reportType) }}
                <span class="text-muted">· {{ triggerLabel(run) }}</span>
              </span>
              <span class="text-muted">{{ formatEpoch(run.computedAt) }}</span>
            </div>
            <div class="mt-1 text-sm text-muted">
              {{ run.findingCount }} finding{{ run.findingCount === 1 ? '' : 's' }}
              <template v-if="run.newFindingCount > 0">
                ·
                <span class="text-primary">{{ run.newFindingCount }} new</span>
              </template>
            </div>
            <p v-if="run.topSummary" class="mt-1 text-sm text-default">
              {{ run.topSummary }}
            </p>
          </li>
        </ul>
      </section>

      <!-- Stories (novelty memory) -->
      <section>
        <h3 class="mb-2 text-xs tracking-wider text-muted uppercase">Stories</h3>
        <p v-if="stories.length === 0" class="text-sm text-muted">
          Nothing has been shown yet — run an analysis to start the memory
        </p>
        <ul v-else class="space-y-2">
          <li
            v-for="story in stories"
            :key="story.fingerprint"
            class="rounded-lg border border-default bg-elevated/30 p-3"
          >
            <p class="text-sm text-default">{{ story.identity }}</p>
            <div class="mt-1 flex flex-wrap items-center gap-x-3 text-sm text-muted">
              <span>{{ story.insight_type.replaceAll('_', ' ') }}</span>
              <span>shown {{ story.shown_count }}×</span>
              <span>first {{ formatEpoch(story.first_shown_at) }}</span>
              <span>last {{ formatEpoch(story.last_shown_at) }}</span>
            </div>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>

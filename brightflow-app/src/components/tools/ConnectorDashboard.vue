<script setup lang="ts">
/**
 * Overview of a connector or upload source: one card per table with what it
 * is (display name, description, rows) and the signals that make it worth
 * opening — when it last changed, how much of it is described, proposals
 * waiting on it, its latest insight run — and a jump into each tool for
 * that table. The signals come from the overview endpoint and refetch
 * when a proposal is decided or an insights run finishes; the listing
 * itself comes from the shared source list. Read-only: every action here
 * is navigation.
 */

import { useQuery } from '@pinia/colada';
import { computed, onBeforeUnmount, ref } from 'vue';
import { useRouter } from 'vue-router';

import { sourceApi } from '@/services/api';
import { isJobEvent } from '@/services/wsGuards';
import { useConnectionStore } from '@/stores/connection';
import { useCurationStore } from '@/stores/curation';
import type { UnifiedSource } from '@/types';
import { changeSummary } from '@/utils/declarationChanges';

import {
  coverageShare,
  coverageText,
  mergeSignals,
  type OverviewTable,
  parseCatalogTime,
  relativeTime,
} from './overview';

const props = defineProps<{
  source: UnifiedSource;
  sourceId: string;
}>();

const router = useRouter();
const connection = useConnectionStore();
const curation = useCurationStore();
curation.initRealtime();

// An insights run finishing changes a card's last-run line, and a decided
// Proposal changes its pending count; both bump the query key.
const runsTick = ref(0);
const stopRunEvents = connection.onMessage('insightsComputed', () => {
  runsTick.value += 1;
});
// A finished sync changes rows and "last changed"; the source list
// Refetches on its own key, the signals on this one.
const stopJobEvents = connection.onMessage('job', (payload) => {
  if (isJobEvent(payload) && payload.job.status !== 'running') {
    runsTick.value += 1;
  }
});
onBeforeUnmount(() => {
  stopRunEvents();
  stopJobEvents();
});

const { data: overview } = useQuery({
  key: () => [
    'source-overview',
    props.sourceId,
    curation.pendingCount,
    curation.version,
    runsTick.value,
  ],
  query: async () => await sourceApi.overview(props.sourceId),
});

const tables = computed<OverviewTable[]>(() =>
  mergeSignals(props.source.tables, overview.value?.tables ?? []),
);

const pendingTotal = computed(() =>
  tables.value.reduce((sum, t) => sum + (t.signals?.pendingProposals ?? 0), 0),
);

function lastChanged(row: OverviewTable): string | null {
  const stamp = row.signals?.updatedAt;
  if (stamp == null) {
    return null;
  }
  const date = parseCatalogTime(stamp);
  return date == null ? null : relativeTime(date);
}

function lastRun(row: OverviewTable): string | null {
  const run = row.signals?.lastInsightRun;
  if (run == null) {
    return null;
  }
  const when = relativeTime(new Date(run.computedAt * 1000));
  const fresh = run.newFindingCount > 0 ? `${run.newFindingCount} new, ` : '';
  return `${fresh}${run.findingCount} finding${run.findingCount === 1 ? '' : 's'} · ${when}`;
}

type TableRoute = 'explore-table' | 'insights-table' | 'semantics-table' | 'textenrichment-table';

function open(route: TableRoute, table: string): void {
  router.push({ name: route, params: { sourceId: props.sourceId, table } });
}
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-4">
    <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
      <div class="flex items-center gap-3">
        <UIcon name="i-lucide-cable" class="h-5 w-5 text-muted" />
        <div>
          <h2 class="text-sm font-semibold text-highlighted">{{ source.name }}</h2>
          <p class="text-sm text-muted">
            <template v-if="source.connectorName">{{ source.connectorName }} connector · </template>
            <template v-if="source.ready">synced and ready</template>
            <template v-else>waiting for the first sync</template>
          </p>
        </div>
      </div>
      <UButton
        v-if="pendingTotal > 0"
        size="md"
        color="warning"
        variant="soft"
        icon="i-lucide-inbox"
        :to="{ name: 'activity' }"
      >
        {{ pendingTotal }} proposal{{ pendingTotal === 1 ? '' : 's' }} to review
      </UButton>
    </div>

    <div v-if="tables.length > 0" class="grid grid-cols-1 gap-3 lg:grid-cols-2">
      <div
        v-for="row in tables"
        :key="row.table.name"
        class="flex flex-col rounded-lg border border-default bg-elevated"
      >
        <div class="flex items-start justify-between gap-3 p-4 pb-3">
          <div class="min-w-0">
            <p class="text-sm font-medium text-highlighted">
              {{ row.table.displayName ?? row.table.name }}
              <span v-if="row.table.displayName" class="font-normal text-muted">
                {{ row.table.name }}
              </span>
            </p>
            <p v-if="row.table.description" class="mt-0.5 text-sm text-default">
              {{ row.table.description }}
            </p>
            <button
              v-else
              type="button"
              class="mt-0.5 text-sm text-muted underline-offset-2 hover:underline"
              @click="open('semantics-table', row.table.name)"
            >
              No description yet — add one
            </button>
          </div>
          <UBadge
            v-if="(row.signals?.pendingProposals ?? 0) > 0"
            size="md"
            color="warning"
            variant="subtle"
            class="flex-shrink-0"
          >
            {{ row.signals?.pendingProposals }} pending
          </UBadge>
        </div>

        <dl class="grid grid-cols-2 gap-x-4 gap-y-2 px-4 pb-3 text-sm">
          <div>
            <dt class="text-muted">Rows</dt>
            <dd class="text-default">
              {{ row.table.numRows == null ? '—' : row.table.numRows.toLocaleString() }}
            </dd>
          </div>
          <div>
            <dt class="text-muted">Last changed</dt>
            <dd class="text-default">{{ lastChanged(row) ?? '—' }}</dd>
          </div>
          <div class="col-span-2">
            <dt class="text-muted">Semantics</dt>
            <dd v-if="row.signals" class="text-default">
              {{ coverageText(row.signals.coverage) }}
              <span class="text-muted"> · {{ row.signals.coverage.withRole }} with a role </span>
              <div class="mt-1 h-1 w-full overflow-hidden rounded bg-accented">
                <div
                  class="h-full rounded bg-primary"
                  :style="{ width: `${Math.round(coverageShare(row.signals.coverage) * 100)}%` }"
                />
              </div>
            </dd>
            <dd v-else class="text-muted">—</dd>
          </div>
          <div class="col-span-2">
            <dt class="text-muted">Insights</dt>
            <dd class="text-default">{{ lastRun(row) ?? 'No run yet' }}</dd>
          </div>
          <div v-if="row.table.lastDeclarationChange" class="col-span-2">
            <dt class="text-muted">Connector</dt>
            <dd class="text-default">{{ changeSummary(row.table.lastDeclarationChange) }}</dd>
          </div>
        </dl>

        <div class="mt-auto flex flex-wrap gap-1 border-t border-default px-2 py-1.5">
          <UButton
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-search"
            @click="open('explore-table', row.table.name)"
          >
            Explore
          </UButton>
          <UButton
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-sparkles"
            @click="open('insights-table', row.table.name)"
          >
            Insights
          </UButton>
          <UButton
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-book-open-text"
            @click="open('semantics-table', row.table.name)"
          >
            Semantics
          </UButton>
          <UButton
            v-if="row.table.enrichable"
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-messages-square"
            @click="open('textenrichment-table', row.table.name)"
          >
            Enrich
          </UButton>
        </div>
      </div>
    </div>
    <p v-else class="text-sm text-muted">No tables synced yet. Run a sync to populate data.</p>
  </div>
</template>

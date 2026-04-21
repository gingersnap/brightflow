<script setup lang="ts">
import { useMutation, useQuery, useQueryCache } from '@pinia/colada';
import { computed, onUnmounted, watch } from 'vue';

import { connectApi } from '@/services/api';
import type { EnrichedSyncRun, UnifiedConnector } from '@/types';

import RunHistoryTable from './RunHistoryTable.vue';
import ScheduleList from './ScheduleList.vue';

const props = defineProps<{
  connectorName?: string;
}>();

const queryCache = useQueryCache();

const { data: connectors } = useQuery({
  key: ['connectors'],
  query: async () => {
    const result = await connectApi.listUnified();
    return result ?? ([] as UnifiedConnector[]);
  },
});

const { data: runs } = useQuery({
  key: ['sync-runs'],
  query: async () => {
    const result = await connectApi.listRuns();
    return result ?? ([] as EnrichedSyncRun[]);
  },
});

function matchesFilter(connector: string): boolean {
  if (!props.connectorName) {
    return true;
  }
  return connector === props.connectorName;
}

const filteredConnectors = computed(() =>
  (connectors.value ?? []).filter((c) => matchesFilter(c.connector)),
);

const filteredRuns = computed(() =>
  (runs.value ?? []).filter((r) => matchesFilter(r.connectorName)),
);

const activeRuns = computed(() =>
  filteredRuns.value.filter((r) => r.status === 'running' || r.status === 'pending'),
);

const historyRuns = computed(() =>
  filteredRuns.value.filter((r) => r.status !== 'running' && r.status !== 'pending').slice(0, 20),
);

const activeRunsExist = computed(() => activeRuns.value.length > 0);

let pollTimer: ReturnType<typeof setTimeout> | null = null;

function startPolling(): void {
  if (pollTimer) {
    return;
  }
  const poll = (): void => {
    queryCache.invalidateQueries({ key: ['connectors'] });
    queryCache.invalidateQueries({ key: ['sync-runs'] });
    pollTimer = setTimeout(poll, 3000);
  };
  pollTimer = setTimeout(poll, 3000);
}

function stopPolling(): void {
  if (pollTimer) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
}

watch(activeRunsExist, (hasActive) => {
  if (hasActive) {
    startPolling();
  } else {
    stopPolling();
  }
});

onUnmounted(() => stopPolling());

const { mutate: syncNow } = useMutation({
  mutation: (name: string) => connectApi.runConnector(name),
  onSettled: () => {
    queryCache.invalidateQueries({ key: ['connectors'] });
    queryCache.invalidateQueries({ key: ['sync-runs'] });
  },
});

const { mutate: deleteSchedule } = useMutation({
  mutation: (jobId: string) => connectApi.deleteSchedule(jobId),
  onSettled: () => queryCache.invalidateQueries({ key: ['connectors'] }),
});
</script>

<template>
  <div class="space-y-6">
    <section v-if="activeRuns.length > 0">
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Active Runs</h3>
      <div class="rounded-lg border border-default bg-default px-3">
        <RunHistoryTable :runs="activeRuns" />
      </div>
    </section>

    <section>
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Schedules</h3>
      <div class="rounded-lg border border-default bg-default px-3">
        <ScheduleList
          :connectors="filteredConnectors"
          @run="syncNow($event)"
          @delete="deleteSchedule($event)"
        />
      </div>
    </section>

    <section>
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Run History</h3>
      <div class="rounded-lg border border-default bg-default px-3">
        <RunHistoryTable :runs="historyRuns" />
      </div>
    </section>
  </div>
</template>

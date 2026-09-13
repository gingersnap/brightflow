<script setup lang="ts">
/**
 * Stateful hub for connector syncing: owns the connector and run queries plus
 * the run/schedule/delete mutations, and polls every 3s only while a run is
 * active so idle pages stay quiet. With `presetName` set it scopes everything
 * to that preset and adds the run-now / schedule quick actions.
 */

import { useMutation, useQuery, useQueryCache } from '@pinia/colada';
import { computed, onBeforeUnmount } from 'vue';

import { connectApi } from '@/services/api';
import { isJobEvent } from '@/services/wsGuards';
import { useConnectionStore } from '@/stores/connection';
import type { EnrichedSyncRun, UnifiedConnector } from '@/types';

import RunHistoryTable from './RunHistoryTable.vue';
import ScheduleList from './ScheduleList.vue';

const SCHEDULE_PRESETS = [
  { label: 'Manual', value: 0 },
  { label: 'Every 1h', value: 3600 },
  { label: 'Every 6h', value: 21_600 },
  { label: 'Every 12h', value: 43_200 },
  { label: 'Daily', value: 86_400 },
  { label: 'Weekly', value: 604_800 },
];

const props = defineProps<{
  presetName?: string;
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

function matchesFilter(name: string): boolean {
  if (!props.presetName) {
    return true;
  }
  return name === props.presetName;
}

const filteredConnectors = computed(() =>
  (connectors.value ?? []).filter((c) => matchesFilter(c.name)),
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

// Every sync run transition arrives as a `job` frame; refetch on each.
const connection = useConnectionStore();
const stopJobEvents = connection.onMessage('job', (payload) => {
  if (isJobEvent(payload) && payload.job.kind === 'connector_sync') {
    queryCache.invalidateQueries({ key: ['connectors'] });
    queryCache.invalidateQueries({ key: ['sync-runs'] });
  }
});
onBeforeUnmount(stopJobEvents);

const { mutate: syncNow, isLoading: triggering } = useMutation({
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

const { mutate: setSchedule, isLoading: savingSchedule } = useMutation({
  mutation: async (input: { name: string; intervalSecs: number }) =>
    await connectApi.scheduleConnector(input.name, input.intervalSecs),
  onSettled: () => {
    queryCache.invalidateQueries({ key: ['connectors'] });
  },
});

const presetConnector = computed(() =>
  props.presetName ? (connectors.value ?? []).find((c) => c.name === props.presetName) : undefined,
);

const currentInterval = computed(() => presetConnector.value?.job?.intervalSecs ?? 0);

const presetIsRunning = computed(() => {
  const last = presetConnector.value?.lastRun;
  return last != null && (last.status === 'running' || last.status === 'pending');
});

function handleIntervalChange(intervalSecs: number): void {
  if (!props.presetName) {
    return;
  }
  setSchedule({ name: props.presetName, intervalSecs });
}

function triggerRunNow(): void {
  if (!props.presetName) {
    return;
  }
  syncNow(props.presetName);
}
</script>

<template>
  <div class="space-y-6">
    <!-- Per-preset quick actions -->
    <section v-if="presetName">
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Sync</h3>
      <div
        class="flex flex-wrap items-center gap-3 rounded-lg border border-default bg-elevated p-3"
      >
        <UButton
          size="md"
          :loading="triggering || presetIsRunning"
          :disabled="triggering || presetIsRunning"
          @click="triggerRunNow"
        >
          <UIcon
            name="i-lucide-play"
            v-if="!(triggering || presetIsRunning)"
            class="mr-1.5 h-3.5 w-3.5"
          />
          {{ presetIsRunning ? 'Running…' : 'Run now' }}
        </UButton>

        <div class="flex items-center gap-2">
          <label class="text-sm text-muted">Schedule</label>
          <USelect
            :model-value="currentInterval"
            :items="SCHEDULE_PRESETS"
            value-key="value"
            :disabled="savingSchedule"
            @update:model-value="handleIntervalChange"
          />
        </div>
      </div>
    </section>

    <section v-if="activeRuns.length > 0">
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Active Runs</h3>
      <div class="rounded-lg border border-default bg-default px-3">
        <RunHistoryTable :runs="activeRuns" @run="syncNow($event)" />
      </div>
    </section>

    <section v-if="!presetName || filteredConnectors.some((c) => c.job?.enabled)">
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
        <RunHistoryTable :runs="historyRuns" @run="syncNow($event)" />
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { useMutation, useQuery, useQueryCache } from '@pinia/colada';
import { Calendar, Play, RefreshCw } from 'lucide-vue-next';
import { computed, onUnmounted, ref, watch } from 'vue';

import { connectApi } from '@/services/api';
import { useConnectStore } from '@/stores/connect';
import type { AvailableConnectorResponse, EnrichedSyncRun, UnifiedConnector } from '@/types';

import NewRunDialog from './NewRunDialog.vue';
import RunHistoryTable from './RunHistoryTable.vue';
import ScheduleList from './ScheduleList.vue';

const connectStore = useConnectStore();
const queryCache = useQueryCache();

const {
  data: connectors,
  isLoading: loadingUnified,
  error: unifiedError,
} = useQuery({
  key: ['connectors'],
  query: async () => {
    const result = await connectApi.listUnified();
    return result ?? ([] as UnifiedConnector[]);
  },
});

const { data: available } = useQuery({
  key: ['connectors-available'],
  query: async () => {
    const result = await connectApi.listAvailable();
    return result ?? ([] as AvailableConnectorResponse[]);
  },
});

const { data: recentRuns } = useQuery({
  key: ['sync-runs'],
  query: async () => {
    const result = await connectApi.listRuns();
    return result ?? ([] as EnrichedSyncRun[]);
  },
});

// Split runs into active vs history
const activeRuns = computed(() =>
  (recentRuns.value ?? []).filter((r) => r.status === 'running' || r.status === 'pending'),
);
const historyRuns = computed(() =>
  (recentRuns.value ?? [])
    .filter((r) => r.status !== 'running' && r.status !== 'pending')
    .slice(0, 20),
);

// Reactive polling: refetch every 3s when active runs exist
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

const connectorList = computed(() => connectors.value ?? []);
const availableList = computed(() => available.value ?? []);
const errorMessage = computed(() => (unifiedError.value ? String(unifiedError.value) : null));

// Dialog mode
type DialogMode = 'run' | 'schedule';
const dialogMode = ref<DialogMode>('run');

function openNewRun(): void {
  dialogMode.value = 'run';
  connectStore.showNewRunDialog = true;
}

function openNewSchedule(): void {
  dialogMode.value = 'schedule';
  connectStore.showNewRunDialog = true;
}

// Mutations
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

function refreshAll(): void {
  queryCache.invalidateQueries({ key: ['connectors'] });
  queryCache.invalidateQueries({ key: ['connectors-available'] });
  queryCache.invalidateQueries({ key: ['sync-runs'] });
}

function onDialogDone(): void {
  refreshAll();
}
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Toolbar -->
    <div class="flex items-center gap-3 border-b border-default bg-default px-4 py-2.5">
      <h2 class="text-sm font-semibold text-highlighted">Data Connectors</h2>
      <div class="flex-1" />
      <UButton variant="ghost" size="sm" :loading="loadingUnified" @click="refreshAll">
        <RefreshCw class="mr-1.5 h-3.5 w-3.5" />
        Refresh
      </UButton>
    </div>

    <!-- Content -->
    <div class="min-h-0 flex-1 overflow-y-auto p-4">
      <!-- Error -->
      <div v-if="errorMessage" class="mb-4 rounded-lg bg-red-500/10 p-3 text-sm text-red-500">
        {{ errorMessage }}
      </div>

      <div class="mx-auto max-w-3xl space-y-6">
        <!-- Action buttons — centered, prominent -->
        <div class="flex items-center justify-center gap-3">
          <UButton size="lg" @click="openNewRun">
            <Play class="mr-1.5 h-4 w-4" />
            New Run
          </UButton>
          <UButton size="lg" variant="outline" @click="openNewSchedule">
            <Calendar class="mr-1.5 h-4 w-4" />
            New Schedule
          </UButton>
        </div>

        <!-- Active Runs section -->
        <section v-if="activeRuns.length > 0">
          <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
            Active Runs
          </h3>
          <div class="rounded-lg border border-default bg-default px-3">
            <RunHistoryTable :runs="activeRuns" />
          </div>
        </section>

        <!-- Schedules section -->
        <section>
          <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Schedules</h3>
          <div class="rounded-lg border border-default bg-default px-3">
            <ScheduleList
              :connectors="connectorList"
              @run="syncNow($event)"
              @delete="deleteSchedule($event)"
            />
          </div>
        </section>

        <!-- Run History section (completed/failed only) -->
        <section>
          <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
            Run History
          </h3>
          <div class="rounded-lg border border-default bg-default px-3">
            <RunHistoryTable :runs="historyRuns" />
          </div>
        </section>
      </div>
    </div>

    <!-- New Run / Schedule Dialog -->
    <NewRunDialog
      :open="connectStore.showNewRunDialog"
      :available="availableList"
      :mode="dialogMode"
      @close="connectStore.showNewRunDialog = false"
      @done="onDialogDone"
    />
  </div>
</template>

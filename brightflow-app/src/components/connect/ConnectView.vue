<script setup lang="ts">
import { useMutation, useQuery, useQueryCache } from '@pinia/colada';
import { RefreshCw } from 'lucide-vue-next';
import { computed, onUnmounted, watch } from 'vue';

import { connectApi } from '@/services/api';
import { useConnectStore } from '@/stores/connect';
import type { UnifiedConnector } from '@/types';

import ConnectorCard from './ConnectorCard.vue';

const connectStore = useConnectStore();
const queryCache = useQueryCache();

const {
  data: connectors,
  isLoading: loading,
  error,
} = useQuery({
  key: ['connectors'],
  query: async () => {
    const result = await connectApi.listUnified();
    return result ?? ([] as UnifiedConnector[]);
  },
});

// Reactive polling: refetch every 3s when active runs exist
const activeRunsExist = computed(
  () =>
    connectors.value?.some(
      (c) => c.lastRun && (c.lastRun.status === 'running' || c.lastRun.status === 'pending'),
    ) ?? false,
);

let pollTimer: ReturnType<typeof setTimeout> | null = null;

function startPolling(): void {
  if (pollTimer) {
    return;
  }
  const poll = (): void => {
    queryCache.invalidateQueries({ key: ['connectors'] });
    if (connectStore.expandedConnector) {
      connectStore.fetchRunHistory(connectStore.expandedConnector);
    }
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
const errorMessage = computed(() => (error.value ? String(error.value) : null));

function isRunning(name: string): boolean {
  const c = connectors.value?.find((conn) => conn.name === name);
  return c?.lastRun != null && (c.lastRun.status === 'running' || c.lastRun.status === 'pending');
}

// Mutations
const { mutate: syncNow } = useMutation({
  mutation: (name: string) => connectApi.runConnector(name),
  onSettled: () => queryCache.invalidateQueries({ key: ['connectors'] }),
});

const { mutate: updateSchedule } = useMutation({
  mutation: ({ name, intervalSecs }: { name: string; intervalSecs: number }) =>
    connectApi.scheduleConnector(name, intervalSecs),
  onSettled: () => queryCache.invalidateQueries({ key: ['connectors'] }),
});

const { mutate: updateToken } = useMutation({
  mutation: ({ name, token }: { name: string; token: string }) =>
    connectApi.updateToken(name, token),
  onSettled: () => queryCache.invalidateQueries({ key: ['connectors'] }),
});
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Toolbar -->
    <div class="flex items-center gap-3 border-b border-default bg-default px-4 py-2.5">
      <h2 class="text-sm font-semibold text-highlighted">Data Connectors</h2>

      <div class="flex-1" />

      <UButton
        variant="ghost"
        size="sm"
        :loading="loading"
        @click="queryCache.invalidateQueries({ key: ['connectors'] })"
      >
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

      <!-- Empty state -->
      <div
        v-if="!loading && connectorList.length === 0"
        class="flex flex-col items-center justify-center py-16 text-center"
      >
        <p class="mb-2 text-muted">No connector configs found</p>
        <p class="text-xs text-muted">
          Place TOML config files in the
          <code class="rounded bg-elevated px-1 py-0.5">configs/</code> directory
        </p>
      </div>

      <!-- Connector cards -->
      <div v-else class="max-w-3xl space-y-3">
        <ConnectorCard
          v-for="connector in connectorList"
          :key="connector.name"
          :connector="connector"
          :running="isRunning(connector.name)"
          :expanded="connectStore.expandedConnector === connector.name"
          :history="connectStore.runHistory.get(connector.name) ?? []"
          @run="syncNow(connector.name)"
          @schedule="updateSchedule({ name: connector.name, intervalSecs: $event })"
          @update-token="updateToken({ name: connector.name, token: $event })"
          @toggle-history="connectStore.toggleHistory(connector.name)"
        />
      </div>
    </div>
  </div>
</template>

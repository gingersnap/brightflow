import { defineStore } from 'pinia';
import { ref, computed, onUnmounted } from 'vue';
import {
  connectApi,
  type UnifiedConnector,
  type SyncRun,
  type ScheduleResponse,
} from '@/services/api';

export const useConnectStore = defineStore('connect', () => {
  const connectors = ref<UnifiedConnector[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const runHistory = ref<Map<string, SyncRun[]>>(new Map());
  const expandedConnector = ref<string | null>(null);
  const activeRuns = ref<Set<string>>(new Set());

  let pollInterval: ReturnType<typeof setInterval> | null = null;

  async function fetchConnectors(): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const result = await connectApi.listUnified();
      connectors.value = result ?? [];

      // Track any currently running connectors
      const running = new Set<string>();
      for (const c of connectors.value) {
        if (c.lastRun && (c.lastRun.status === 'running' || c.lastRun.status === 'pending')) {
          running.add(c.name);
        }
      }
      activeRuns.value = running;

      if (running.size > 0) {
        startPolling();
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load connectors';
    } finally {
      loading.value = false;
    }
  }

  async function syncNow(name: string): Promise<void> {
    error.value = null;

    try {
      const result = await connectApi.runConnector(name);
      if (result) {
        activeRuns.value = new Set([...activeRuns.value, name]);
        startPolling();
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to start connector';
    }
  }

  async function updateSchedule(
    name: string,
    intervalSecs: number,
  ): Promise<ScheduleResponse | null> {
    error.value = null;
    try {
      const result = await connectApi.scheduleConnector(name, intervalSecs);
      // Refresh to pick up new job info
      await fetchConnectors();
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to update schedule';
      return null;
    }
  }

  async function updateToken(name: string, token: string): Promise<void> {
    error.value = null;
    try {
      await connectApi.updateToken(name, token);
      await fetchConnectors();
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to update token';
    }
  }

  async function fetchRunHistory(name: string): Promise<void> {
    try {
      const result = await connectApi.listConnectorRuns(name);
      const updated = new Map(runHistory.value);
      updated.set(name, result ?? []);
      runHistory.value = updated;
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load run history';
    }
  }

  function toggleHistory(name: string): void {
    if (expandedConnector.value === name) {
      expandedConnector.value = null;
    } else {
      expandedConnector.value = name;
      fetchRunHistory(name);
    }
  }

  const hasActiveRuns = computed(() => activeRuns.value.size > 0);

  function isRunning(name: string): boolean {
    return activeRuns.value.has(name);
  }

  function startPolling(): void {
    if (pollInterval) return;
    pollInterval = setInterval(async () => {
      await fetchConnectors();

      // Refresh history for expanded connector if it was running
      if (expandedConnector.value && activeRuns.value.has(expandedConnector.value)) {
        await fetchRunHistory(expandedConnector.value);
      }

      if (!hasActiveRuns.value) {
        stopPolling();
      }
    }, 3000);
  }

  function stopPolling(): void {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  onUnmounted(() => {
    stopPolling();
  });

  return {
    connectors,
    loading,
    error,
    runHistory,
    expandedConnector,
    hasActiveRuns,
    fetchConnectors,
    syncNow,
    updateToken,
    updateSchedule,
    fetchRunHistory,
    toggleHistory,
    isRunning,
    stopPolling,
  };
});

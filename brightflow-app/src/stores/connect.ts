import { defineStore } from 'pinia';
import { ref, onUnmounted } from 'vue';
import {
  connectApi,
  type ConnectorInfo,
  type ConnectorRun,
  type ScheduleResponse,
} from '@/services/api';

export const useConnectStore = defineStore('connect', () => {
  const connectors = ref<ConnectorInfo[]>([]);
  const runs = ref<ConnectorRun[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const activeRunId = ref<string | null>(null);

  let pollInterval: ReturnType<typeof setInterval> | null = null;

  async function fetchConnectors(): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const result = await connectApi.listConnectors();
      connectors.value = result ?? [];
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load connectors';
    } finally {
      loading.value = false;
    }
  }

  async function fetchRuns(): Promise<void> {
    try {
      const result = await connectApi.listRuns();
      runs.value = result ?? [];
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load runs';
    }
  }

  async function runConnector(name: string, only?: string): Promise<void> {
    error.value = null;

    try {
      const result = await connectApi.runConnector(name, only);
      if (result) {
        activeRunId.value = result.run_id;
        startPolling();
        // Refresh runs list
        await fetchRuns();
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to start connector';
    }
  }

  function startPolling(): void {
    stopPolling();
    pollInterval = setInterval(async () => {
      await fetchRuns();

      // Check if active run is done
      if (activeRunId.value) {
        const active = runs.value.find((r) => r.id === activeRunId.value);
        if (active && (active.status === 'completed' || active.status === 'failed')) {
          activeRunId.value = null;
          stopPolling();
        }
      }
    }, 2000);
  }

  function stopPolling(): void {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  function isRunning(connectorName: string): boolean {
    return runs.value.some(
      (r) => r.connector === connectorName && (r.status === 'pending' || r.status === 'running'),
    );
  }

  function latestRun(connectorName: string): ConnectorRun | undefined {
    return runs.value.find((r) => r.connector === connectorName);
  }

  async function scheduleConnector(
    name: string,
    intervalSecs: number,
  ): Promise<ScheduleResponse | null> {
    error.value = null;
    try {
      return await connectApi.scheduleConnector(name, intervalSecs);
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to schedule connector';
      return null;
    }
  }

  onUnmounted(() => {
    stopPolling();
  });

  return {
    connectors,
    runs,
    loading,
    error,
    activeRunId,
    fetchConnectors,
    fetchRuns,
    runConnector,
    isRunning,
    latestRun,
    scheduleConnector,
    stopPolling,
  };
});

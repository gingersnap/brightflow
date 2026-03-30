import { defineStore } from 'pinia';
import { ref } from 'vue';
import { connectApi } from '@/services/api';
import type { SyncRun } from '@/types';

/**
 * Thin store for connect UI state only.
 * Data fetching is handled by Pinia Colada in ConnectView.vue.
 */
export const useConnectStore = defineStore('connect', () => {
  const runHistory = ref<Map<string, SyncRun[]>>(new Map());
  const expandedConnector = ref<string | null>(null);

  async function fetchRunHistory(name: string): Promise<void> {
    const result = await connectApi.listConnectorRuns(name);
    const updated = new Map(runHistory.value);
    updated.set(name, result ?? []);
    runHistory.value = updated;
  }

  function toggleHistory(name: string): void {
    if (expandedConnector.value === name) {
      expandedConnector.value = null;
    } else {
      expandedConnector.value = name;
      fetchRunHistory(name);
    }
  }

  function reset(): void {
    expandedConnector.value = null;
    runHistory.value = new Map();
  }

  return {
    runHistory,
    expandedConnector,
    fetchRunHistory,
    toggleHistory,
    reset,
  };
});

/**
 * The loaded dataset: its columns and its identity.
 *
 * Loading a new dataset resets UI state and seeds the results store from here,
 * because both are keyed to the previous table's columns and would otherwise
 * surface stale state under a new dataset's name. This is not the whole reset —
 * `resetAllStores` covers the rest — so treat it as the minimum this store owes
 * its own consumers, not as a guarantee that nothing stale survives anywhere.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { datasetApi } from '@/services/api';
import { createLogger } from '@/services/logger';
import type { ColumnInfo, LoadTableResponse } from '@/types';

const log = createLogger('Dataset');

import { useQueryStore } from './query';
import { useResultsStore } from './results';
import { useUiStore } from './ui';

export const useDatasetStore = defineStore('dataset', () => {
  // State - current dataset
  const id = ref('default');
  const name = ref<string | null>(null);
  const rowCount = ref<number | null>(null);
  const columns = ref<ColumnInfo[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  // Computed
  const numericColumns = computed(() =>
    columns.value.filter((c) => ['int', 'float', 'decimal', 'number'].includes(c.dtype)),
  );

  const hasData = computed(() => columns.value.length > 0);

  // Set state from a LoadTableResponse (REST-based, no WS needed)
  function setFromLoadResponse(response: LoadTableResponse): void {
    id.value = response.id;
    name.value = response.name;
    rowCount.value = response.rowCount;
    columns.value = response.columns;
    loading.value = false;
    error.value = null;

    // Reset UI state for new dataset
    const uiStore = useUiStore();
    uiStore.resetForNewDataset();

    // Load initial table data via REST
    void loadInitialDataRest();
  }

  function reset(): void {
    id.value = 'default';
    name.value = null;
    rowCount.value = null;
    columns.value = [];
    error.value = null;
  }

  // Load initial table data via REST (LIMIT 100)
  async function loadInitialDataRest(): Promise<void> {
    const resultsStore = useResultsStore();
    const queryStore = useQueryStore();

    if (columns.value.length === 0) {
      return;
    }

    resultsStore.setLoading(true);

    try {
      const ops: { type: string; n?: number }[] = [];
      if (queryStore.limit > 0) {
        ops.push({ n: queryStore.limit, type: 'limit' });
      }

      const result = await datasetApi.query(id.value, ops);
      if (result) {
        resultsStore.setTableResults(result);
      }
    } catch {
      log.error('Failed to load initial data');
      resultsStore.setError('Failed to load data');
    }
  }

  return {
    // Current dataset state
    id,
    name,
    rowCount,
    columns,
    loading,
    error,
    // Computed
    numericColumns,
    hasData,
    // Actions
    setFromLoadResponse,
    reset,
  };
});

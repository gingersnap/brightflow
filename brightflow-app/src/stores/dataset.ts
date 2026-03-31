import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { datasetApi } from '@/services/api';
import type { ColumnInfo, LoadTableResponse } from '@/types';

import { useQueryStore } from './query';
import { useResultsStore } from './results';
import { useUiStore } from './ui';

// Dataset summary from list endpoint
export interface DatasetSummary {
  id: string;
  name: string;
  rowCount?: number | null;
  columnCount?: number | null;
}

export const useDatasetStore = defineStore('dataset', () => {
  // State - current dataset
  const id = ref('default');
  const name = ref<string | null>(null);
  const rowCount = ref<number | null>(null);
  const columnCount = ref<number | null>(null);
  const columns = ref<ColumnInfo[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  // State - available datasets
  const availableDatasets = ref<DatasetSummary[]>([]);
  const loadingList = ref(false);

  // Computed
  const numericColumns = computed(() =>
    columns.value.filter((c) => ['int', 'float', 'decimal', 'number'].includes(c.dtype)),
  );

  const stringColumns = computed(() =>
    columns.value.filter((c) => ['string', 'text', 'varchar'].includes(c.dtype)),
  );

  const hasData = computed(() => columns.value.length > 0);

  // Set state from a LoadTableResponse (REST-based, no WS needed)
  function setFromLoadResponse(response: LoadTableResponse): void {
    id.value = response.id;
    name.value = response.name;
    rowCount.value = response.rowCount;
    columnCount.value = response.columnCount;
    columns.value = response.columns;
    loading.value = false;
    error.value = null;

    // Reset UI state for new dataset
    const uiStore = useUiStore();
    uiStore.resetForNewDataset();

    // Load initial table data via REST
    void loadInitialDataRest();
  }

  function getColumnByName(columnName: string): ColumnInfo | undefined {
    return columns.value.find((c) => c.name === columnName);
  }

  function getColumnType(columnName: string): string {
    const column = getColumnByName(columnName);
    return column?.dtype ?? 'string';
  }

  function reset(): void {
    id.value = 'default';
    name.value = null;
    rowCount.value = null;
    columnCount.value = null;
    columns.value = [];
    error.value = null;
  }

  // Fetch list of available datasets from REST API
  async function fetchAvailableDatasets(): Promise<void> {
    loadingList.value = true;
    try {
      const datasets = await datasetApi.list();
      if (datasets) {
        availableDatasets.value = datasets.map((d) => ({
          columnCount: d.columnCount,
          id: d.id,
          name: d.name,
          rowCount: d.rowCount,
        }));
      }
      // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the store ref
    } catch (err) {
      console.error('Failed to fetch datasets:', err);
    } finally {
      loadingList.value = false;
    }
  }

  // Switch to a different dataset
  function switchDataset(datasetId: string): void {
    if (datasetId === id.value) {
      return;
    }

    // Update id immediately for UI responsiveness
    id.value = datasetId;

    // Reset query state when switching datasets
    const queryStore = useQueryStore();
    const resultsStore = useResultsStore();

    queryStore.reset();
    resultsStore.clear();
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
        resultsStore.setTableResults(result as unknown as Record<string, unknown>);
      }
      // oxlint-disable-next-line unicorn/catch-error-name -- `error` shadows the store ref
    } catch (err) {
      console.error('[Dataset] Failed to load initial data:', err);
      const msg = err instanceof Error ? err.message : 'Failed to load data';
      resultsStore.setError(msg);
    }
  }

  return {
    // Current dataset state
    id,
    name,
    rowCount,
    columnCount,
    columns,
    loading,
    error,
    // Available datasets
    availableDatasets,
    loadingList,
    // Computed
    numericColumns,
    stringColumns,
    hasData,
    // Actions
    setFromLoadResponse,
    fetchAvailableDatasets,
    switchDataset,
    getColumnByName,
    getColumnType,
    reset,
  };
});

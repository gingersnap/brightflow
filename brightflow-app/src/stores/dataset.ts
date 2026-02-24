import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { useUiStore } from './ui';
import { useResultsStore } from './results';
import { useQueryStore } from './query';
import { datasetApi, type LoadTableResponse } from '@/services/api';
import type { Column } from '@/types';

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
  const columns = ref<Column[]>([]);
  const dataMode = ref<string | null>(null);
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
    dataMode.value = response.dataMode ?? 'memory';
    loading.value = false;
    error.value = null;

    // Reset UI state for new dataset
    const uiStore = useUiStore();
    uiStore.resetForNewDataset();

    // Load initial table data via REST
    loadInitialDataRest();
  }

  function getColumnByName(columnName: string): Column | undefined {
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
    dataMode.value = null;
    error.value = null;
  }

  // Fetch list of available datasets from REST API
  async function fetchAvailableDatasets(): Promise<void> {
    loadingList.value = true;
    try {
      const datasets = await datasetApi.list();
      if (datasets) {
        availableDatasets.value = datasets.map((d) => ({
          id: d.id,
          name: d.name,
          rowCount: d.rowCount,
          columnCount: d.columnCount,
        }));
      }
    } catch (e) {
      console.error('Failed to fetch datasets:', e);
    } finally {
      loadingList.value = false;
    }
  }

  // Switch to a different dataset
  function switchDataset(datasetId: string): void {
    if (datasetId === id.value) return;

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

    if (columns.value.length === 0) return;

    resultsStore.setLoading(true);

    try {
      const ops: Array<{ type: string; n?: number }> = [];
      if (queryStore.limit > 0) {
        ops.push({ type: 'limit', n: queryStore.limit });
      }

      const result = await datasetApi.query(id.value, ops);
      if (result) {
        resultsStore.setTableResults(result as unknown as Record<string, unknown>);
      }
    } catch (e) {
      console.error('[Dataset] Failed to load initial data:', e);
      const msg = e instanceof Error ? e.message : 'Failed to load data';
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
    dataMode,
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

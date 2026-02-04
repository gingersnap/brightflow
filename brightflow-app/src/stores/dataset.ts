import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { useConnectionStore } from './connection';
import { useUiStore } from './ui';
import { useResultsStore } from './results';
import { useQueryStore } from './query';
import type { Column, MetadataMessage, ErrorMessage, WsMessage, LimitOperation } from '@/types';

export const useDatasetStore = defineStore('dataset', () => {
  // State
  const id = ref('default');
  const name = ref<string | null>(null);
  const rowCount = ref<number | null>(null);
  const columnCount = ref<number | null>(null);
  const columns = ref<Column[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  // Computed
  const numericColumns = computed(() =>
    columns.value.filter((c) => ['int', 'float', 'decimal', 'number'].includes(c.dtype)),
  );

  const stringColumns = computed(() =>
    columns.value.filter((c) => ['string', 'text', 'varchar'].includes(c.dtype)),
  );

  const hasData = computed(() => columns.value.length > 0);

  // Actions
  function fetchMetadata(datasetId = 'default'): void {
    const connectionStore = useConnectionStore();

    if (!connectionStore.isConnected) {
      error.value = 'Not connected';
      loading.value = false;
      return;
    }

    loading.value = true;
    error.value = null;

    // Timeout after 10 seconds
    const timeout = setTimeout(() => {
      if (loading.value) {
        loading.value = false;
        error.value = 'Request timed out';
        unsubscribe();
        unsubscribeError();
      }
    }, 10000);

    // Register one-time handler for metadata response
    const unsubscribe = connectionStore.onMessage('metadata', (message: WsMessage) => {
      const metaMsg = message as MetadataMessage;
      // Backend uses snake_case: dataset_id, row_count
      if (metaMsg.dataset_id === datasetId) {
        clearTimeout(timeout);
        id.value = metaMsg.dataset_id;
        name.value = metaMsg.name;
        rowCount.value = metaMsg.row_count;
        columnCount.value = metaMsg.columns?.length ?? 0;
        columns.value = metaMsg.columns ?? [];
        loading.value = false;
        unsubscribe();
        unsubscribeError();

        // Reset UI state for new dataset
        const uiStore = useUiStore();
        uiStore.resetForNewDataset();

        // Load raw table data
        loadInitialData();
      }
    });

    // Handle errors
    const unsubscribeError = connectionStore.onMessage('error', (message: WsMessage) => {
      const errMsg = message as ErrorMessage;
      clearTimeout(timeout);
      error.value = errMsg.message;
      loading.value = false;
      unsubscribe();
      unsubscribeError();
    });

    // Send request
    connectionStore.send({
      type: 'getMetadata',
      datasetId,
    });
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
    error.value = null;
  }

  // Load initial table data after metadata is received
  function loadInitialData(): void {
    const connectionStore = useConnectionStore();
    const resultsStore = useResultsStore();
    const queryStore = useQueryStore();

    if (!connectionStore.isConnected || columns.value.length === 0) {
      return;
    }

    resultsStore.setLoading(true);

    const unsubscribeResult = connectionStore.onMessage('queryResult', (message: WsMessage) => {
      resultsStore.setTableResults(message);
      unsubscribeResult();
      unsubscribeError();
    });

    const unsubscribeError = connectionStore.onMessage('error', (message: WsMessage) => {
      const errMsg = message as ErrorMessage;
      resultsStore.setError(errMsg.message);
      unsubscribeResult();
      unsubscribeError();
    });

    // Query with limit from query store (default 100)
    const ops: LimitOperation[] = [];
    if (queryStore.limit > 0) {
      ops.push({ type: 'limit', n: queryStore.limit });
    }

    connectionStore.send({
      type: 'query',
      datasetId: id.value,
      operations: ops,
    });
  }

  return {
    id,
    name,
    rowCount,
    columnCount,
    columns,
    loading,
    error,
    numericColumns,
    stringColumns,
    hasData,
    fetchMetadata,
    getColumnByName,
    getColumnType,
    reset,
  };
});

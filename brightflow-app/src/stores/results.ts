import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { Column } from '@/types';

// Flexible type to accept both WsMessage and explicit result data
interface ResultData {
  columns?: Column[];
  rows?: unknown[][];
  rowCount?: number;
  row_count?: number;
  totalRows?: number;
  total_rows?: number;
  executionTimeMs?: number;
  execution_time_ms?: number;
  [key: string]: unknown; // Allow additional properties from WsMessage
}

type ResultType = 'table' | 'pivot';

export const useResultsStore = defineStore('results', () => {
  // Table data (raw data)
  const tableColumns = ref<Column[]>([]);
  const tableRows = ref<unknown[][]>([]);
  const tableRowCount = ref(0);
  const tableTotalRows = ref(0);
  const tableExecutionTimeMs = ref<number | null>(null);

  // Pivot data (aggregated data)
  const pivotColumns = ref<Column[]>([]);
  const pivotRows = ref<unknown[][]>([]);
  const pivotRowCount = ref(0);
  const pivotTotalRows = ref(0);
  const pivotExecutionTimeMs = ref<number | null>(null);

  // Shared state
  const loading = ref(false);
  const error = ref<string | null>(null);

  // Legacy computed (for backwards compatibility) - these return table data by default
  const columns = computed(() => tableColumns.value);
  const rows = computed(() => tableRows.value);
  const rowCount = computed(() => tableRowCount.value);
  const totalRows = computed(() => tableTotalRows.value);
  const executionTimeMs = computed(() => tableExecutionTimeMs.value);

  // Computed
  const hasTableResults = computed(() => tableRows.value.length > 0);
  const hasPivotResults = computed(() => pivotRows.value.length > 0);
  const hasResults = computed(() => hasTableResults.value || hasPivotResults.value);
  const isEmpty = computed(() => !loading.value && !error.value && !hasResults.value);
  const isTruncated = computed(() => tableRowCount.value < tableTotalRows.value);

  const columnNames = computed(() => tableColumns.value.map((c) => c.name));

  const columnTypes = computed(() => {
    return tableColumns.value.reduce<Record<string, string>>((acc, col) => {
      acc[col.name] = col.dtype;
      return acc;
    }, {});
  });

  // Actions
  function setLoading(isLoading: boolean): void {
    loading.value = isLoading;
    if (isLoading) {
      error.value = null;
    }
  }

  function setResults(data: ResultData, type: ResultType = 'table'): void {
    const cols = data.columns ?? [];
    const rowsData = data.rows ?? [];
    const count = data.rowCount ?? data.row_count ?? rowsData.length;
    const total = data.totalRows ?? data.total_rows ?? count;
    const time = data.executionTimeMs ?? data.execution_time_ms ?? null;

    if (type === 'pivot') {
      pivotColumns.value = cols;
      pivotRows.value = rowsData;
      pivotRowCount.value = count;
      pivotTotalRows.value = total;
      pivotExecutionTimeMs.value = time;
    } else {
      tableColumns.value = cols;
      tableRows.value = rowsData;
      tableRowCount.value = count;
      tableTotalRows.value = total;
      tableExecutionTimeMs.value = time;
    }

    loading.value = false;
    error.value = null;
  }

  function setTableResults(data: ResultData): void {
    setResults(data, 'table');
  }

  function setPivotResults(data: ResultData): void {
    setResults(data, 'pivot');
  }

  function setError(err: string | { message?: string }): void {
    error.value = typeof err === 'string' ? err : (err.message ?? 'Query failed');
    loading.value = false;
  }

  function clear(): void {
    tableColumns.value = [];
    tableRows.value = [];
    tableRowCount.value = 0;
    tableTotalRows.value = 0;
    tableExecutionTimeMs.value = null;
    pivotColumns.value = [];
    pivotRows.value = [];
    pivotRowCount.value = 0;
    pivotTotalRows.value = 0;
    pivotExecutionTimeMs.value = null;
    error.value = null;
  }

  function clearPivot(): void {
    pivotColumns.value = [];
    pivotRows.value = [];
    pivotRowCount.value = 0;
    pivotTotalRows.value = 0;
    pivotExecutionTimeMs.value = null;
  }

  // Helper to escape CSV cell
  function escapeCsvCell(cell: unknown): string {
    if (cell === null || cell === undefined) return '';
    const str = String(cell);
    // Escape quotes and wrap in quotes if contains comma, quote, or newline
    if (str.includes(',') || str.includes('"') || str.includes('\n')) {
      return `"${str.replace(/"/g, '""')}"`;
    }
    return str;
  }

  // Export to CSV - supports both table and pivot data
  function exportCsv(type: ResultType = 'table'): void {
    const cols = type === 'pivot' ? pivotColumns.value : tableColumns.value;
    const data = type === 'pivot' ? pivotRows.value : tableRows.value;

    if (cols.length === 0 || data.length === 0) return;

    const headers = cols.map((c) => escapeCsvCell(c.name)).join(',');
    const csvRows = data.map((row) => row.map((cell) => escapeCsvCell(cell)).join(','));

    const csv = [headers, ...csvRows].join('\n');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);

    const link = document.createElement('a');
    link.href = url;
    link.download = `${type}-results-${Date.now()}.csv`;
    link.click();

    URL.revokeObjectURL(url);
  }

  return {
    // Table data
    tableColumns,
    tableRows,
    tableRowCount,
    tableTotalRows,
    tableExecutionTimeMs,
    // Pivot data
    pivotColumns,
    pivotRows,
    pivotRowCount,
    pivotTotalRows,
    pivotExecutionTimeMs,
    // Legacy (backwards compat)
    columns,
    rows,
    rowCount,
    totalRows,
    executionTimeMs,
    // Shared
    loading,
    error,
    hasTableResults,
    hasPivotResults,
    hasResults,
    isEmpty,
    isTruncated,
    columnNames,
    columnTypes,
    // Actions
    setLoading,
    setResults,
    setTableResults,
    setPivotResults,
    setError,
    clear,
    clearPivot,
    exportCsv,
  };
});

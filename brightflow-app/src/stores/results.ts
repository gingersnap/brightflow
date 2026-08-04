/**
 * The current query result sets (table and pivot) and their metadata.
 *
 * Accepts both WebSocket frames and direct REST responses through one shape,
 * so downstream views do not care which path produced the rows. CSV export
 * lives with the button that triggers it (ResultsPanel), not here.
 */

import { defineStore } from 'pinia';
import { computed, ref, type Ref } from 'vue';

import type { ColumnInfo } from '@/types';

// Flexible type to accept WsMessage data and explicit result data
interface ResultData {
  columns?: ColumnInfo[];
  rows?: unknown[][];
  rowCount?: number;
  totalRows?: number;
  executionTimeMs?: number;
  [key: string]: unknown;
}

export type ResultType = 'table' | 'pivot';

/** One result set: columns, rows, and the counts/timing that came with it. */
export interface ResultSet {
  columns: ColumnInfo[];
  rows: unknown[][];
  rowCount: number;
  totalRows: number;
  executionTimeMs: number | null;
}

function emptyResultSet(): ResultSet {
  return { columns: [], executionTimeMs: null, rowCount: 0, rows: [], totalRows: 0 };
}

export const useResultsStore = defineStore('results', () => {
  const table = ref<ResultSet>(emptyResultSet());
  const pivot = ref<ResultSet>(emptyResultSet());
  const byType: Record<ResultType, Ref<ResultSet>> = { pivot, table };

  // Shared state
  const loading = ref(false);
  const error = ref<string | null>(null);

  // Computed
  const hasTableResults = computed(() => table.value.rows.length > 0);
  const hasPivotResults = computed(() => pivot.value.rows.length > 0);
  const hasResults = computed(() => hasTableResults.value || hasPivotResults.value);

  // Actions
  function setLoading(isLoading: boolean): void {
    loading.value = isLoading;
    if (isLoading) {
      error.value = null;
    }
  }

  function setResults(type: ResultType, data: ResultData): void {
    const rows = data.rows ?? [];
    const rowCount = data.rowCount ?? rows.length;
    byType[type].value = {
      columns: data.columns ?? [],
      executionTimeMs: data.executionTimeMs ?? null,
      rowCount,
      rows,
      totalRows: data.totalRows ?? rowCount,
    };
    loading.value = false;
    error.value = null;
  }

  function setError(err: string | { message?: string }): void {
    error.value = typeof err === 'string' ? err : (err.message ?? 'Query failed');
    loading.value = false;
  }

  function clear(): void {
    table.value = emptyResultSet();
    pivot.value = emptyResultSet();
    error.value = null;
  }

  return {
    // Result sets
    table,
    pivot,
    // Shared
    loading,
    error,
    hasTableResults,
    hasPivotResults,
    hasResults,
    // Actions
    setLoading,
    setResults,
    setError,
    clear,
  };
});

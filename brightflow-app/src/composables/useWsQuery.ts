/**
 * Builds and executes queries over the WebSocket connection.
 *
 * Assembles the operation chain from the query, pivot, and UI stores so
 * callers do not each reimplement that translation. Every query carries a
 * crypto.randomUUID() correlation id and only the matching queryResult
 * resolves it, so two in-flight queries can no longer swap answers.
 * Uncorrelated errors (parse failures carry no id) reject whatever is
 * pending — better a spurious error than a hang.
 */

import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { usePivotStore } from '@/stores/pivot';
import { filterOperations, useQueryStore } from '@/stores/query';
import { useResultsStore } from '@/stores/results';
import { useUiStore } from '@/stores/ui';
import type { Operation } from '@/types/generated';
import { buildPivotOperations } from '@/utils/buildOperations';

/**
 * WebSocket query execution composable
 */
export function useWsQuery() {
  const connectionStore = useConnectionStore();
  const datasetStore = useDatasetStore();
  const queryStore = useQueryStore();
  const resultsStore = useResultsStore();
  const pivotStore = usePivotStore();
  const uiStore = useUiStore();

  /**
   * Build operations for table view (filters + limit)
   */
  function buildTableOperations(): Operation[] {
    const ops: Operation[] = [];

    // Add filters
    ops.push(...filterOperations(queryStore.filters));

    // Add limit (0 means no limit)
    if (queryStore.limit > 0) {
      ops.push({ n: queryStore.limit, type: 'limit' });
    }

    return ops;
  }

  /**
   * Send one query and resolve with its correlated result frame.
   */
  function sendQuery(operations: Operation[]): Promise<Record<string, unknown>> {
    const requestId = crypto.randomUUID();
    return new Promise((resolve, reject) => {
      const unsubscribeResult = connectionStore.onMessage(
        'queryResult',
        (message: Record<string, unknown>) => {
          if (message['requestId'] !== requestId) {
            return; // Another (stale) query's answer.
          }
          cleanup();
          resolve(message);
        },
      );

      const unsubscribeError = connectionStore.onMessage(
        'error',
        (message: Record<string, unknown>) => {
          if (message['requestId'] !== undefined && message['requestId'] !== requestId) {
            return; // Another query's failure.
          }
          cleanup();
          const msg = message['message'];
          reject(new Error(typeof msg === 'string' ? msg : 'Query failed'));
        },
      );

      function cleanup(): void {
        unsubscribeResult();
        unsubscribeError();
      }

      connectionStore.send({
        datasetId: datasetStore.id,
        operations,
        requestId,
        type: 'query',
      });
    });
  }

  /**
   * Load table data with current filters and limit
   */
  function loadTableData(): void {
    if (!canExecute()) {
      return;
    }

    resultsStore.setLoading(true);
    sendQuery(buildTableOperations())
      .then((message) => {
        resultsStore.setResults('table', message);
      })
      .catch((error: unknown) => {
        resultsStore.setError(error instanceof Error ? error.message : 'Query failed');
      });
  }

  /**
   * Build operations based on current configuration: the pivot chain when a
   * pivot is configured (regardless of view mode), else the query store's.
   */
  function buildOperations(): Operation[] {
    if (!pivotStore.isConfigured) {
      return queryStore.operations;
    }
    return buildPivotOperations({
      columnFields: pivotStore.columnFields,
      filters: queryStore.filters,
      filtersEnabled: queryStore.sections.filter.enabled,
      limit: queryStore.limit,
      limitEnabled: queryStore.sections.limit.enabled,
      rowFields: pivotStore.rowFields,
      sortBy: queryStore.sortBy,
      sortDescending: queryStore.sortDescending,
      sortEnabled: queryStore.sections.sort.enabled,
      valueFields: pivotStore.valueFields,
    });
  }

  /**
   * Execute pivot query (only when pivot is configured with values)
   */
  function executePivot(): void {
    if (!connectionStore.isConnected) {
      resultsStore.setError('Not connected to server');
      return;
    }

    if (!datasetStore.hasData) {
      resultsStore.setError('No dataset loaded');
      return;
    }

    if (!pivotStore.isConfigured) {
      return;
    }

    resultsStore.setLoading(true);
    sendQuery(buildOperations())
      .then((message) => {
        resultsStore.setResults('pivot', message);
        uiStore.onPivotResults();
      })
      .catch((error: unknown) => {
        resultsStore.setError(error instanceof Error ? error.message : 'Query failed');
      });
  }

  /**
   * Check if query can be executed
   */
  function canExecute(): boolean {
    return connectionStore.isConnected && datasetStore.hasData;
  }

  return {
    buildOperations,
    buildTableOperations,
    canExecute,
    executePivot,
    loadTableData,
  };
}

import { useConnectionStore } from '@/stores/connection';
import { useDatasetStore } from '@/stores/dataset';
import { useQueryStore } from '@/stores/query';
import { useResultsStore } from '@/stores/results';
import { usePivotStore } from '@/stores/pivot';
import { useUiStore } from '@/stores/ui';
import type { Operation, Aggregation as AggFn, FilterOp } from '@/types/generated';

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
    if (queryStore.filters.length > 0) {
      queryStore.filters.forEach((filter) => {
        if (filter.column && filter.op) {
          ops.push({
            type: 'filter',
            column: filter.column,
            op: filter.op as FilterOp,
            value: ['isNull', 'isNotNull'].includes(filter.op) ? null : filter.value,
          });
        }
      });
    }

    // Add limit (0 means no limit)
    if (queryStore.limit > 0) {
      ops.push({ type: 'limit', n: queryStore.limit });
    }

    return ops;
  }

  /**
   * Load table data with current filters and limit
   */
  function loadTableData(): void {
    if (!connectionStore.isConnected || !datasetStore.hasData) {
      return;
    }

    const operations = buildTableOperations();

    resultsStore.setLoading(true);

    const unsubscribeResult = connectionStore.onMessage(
      'queryResult',
      (message: Record<string, unknown>) => {
        resultsStore.setTableResults(message);
        unsubscribeResult();
        unsubscribeError();
      },
    );

    const unsubscribeError = connectionStore.onMessage(
      'error',
      (message: Record<string, unknown>) => {
        resultsStore.setError((message['message'] as string) ?? 'Query failed');
        unsubscribeResult();
        unsubscribeError();
      },
    );

    connectionStore.send({
      type: 'query',
      datasetId: datasetStore.id,
      operations,
    });
  }

  /**
   * Build operations based on current configuration
   */
  function buildOperations(): Operation[] {
    // If pivot is configured, use pivot operation (regardless of view mode)
    if (pivotStore.isConfigured) {
      const ops: Operation[] = [];

      // Add any filters from query store
      if (queryStore.sections.filter.enabled && queryStore.filters.length > 0) {
        queryStore.filters.forEach((filter) => {
          if (filter.column && filter.op) {
            ops.push({
              type: 'filter',
              column: filter.column,
              op: filter.op as FilterOp,
              value: ['isNull', 'isNotNull'].includes(filter.op) ? null : filter.value,
            });
          }
        });
      }

      // Add pivot operation
      if (pivotStore.valueFields.length > 0) {
        const valueField = pivotStore.valueFields[0];
        if (!valueField) return ops;

        const rowCols = pivotStore.rowFields.map((f) => f.column);
        const colField =
          pivotStore.columnFields.length > 0 ? (pivotStore.columnFields[0]?.column ?? null) : null;
        const aggFunc = (valueField.aggregation ?? 'count') as AggFn;

        // Determine the best operation based on configuration
        if (rowCols.length === 0 && !colField) {
          console.warn(
            '[useWsQuery] Pivot has values but no rows/columns - waiting for UI to auto-add',
          );
          return ops;
        }

        if (!colField) {
          // Only rows, no column pivot - use groupBy
          ops.push({
            type: 'groupBy',
            by: rowCols,
            aggs: [{ column: valueField.column, function: aggFunc, alias: aggFunc }],
          });
        } else if (rowCols.length === 0) {
          // Only columns (no rows) - group by the column field
          ops.push({
            type: 'groupBy',
            by: [colField],
            aggs: [{ column: valueField.column, function: aggFunc, alias: aggFunc }],
          });
        } else {
          // Full pivot with both rows and columns
          ops.push({
            type: 'pivot',
            index: rowCols,
            columns: colField,
            values: valueField.column,
            agg: aggFunc,
          });
        }

        const lastOp = ops[ops.length - 1];
        console.log('[useWsQuery] Pivot/GroupBy operation:', lastOp);
      }

      // Add sort
      if (queryStore.sections.sort.enabled && queryStore.sortBy) {
        ops.push({
          type: 'sort',
          by: queryStore.sortBy,
          descending: queryStore.sortDescending,
        });
      }

      // Add limit
      if (queryStore.sections.limit.enabled && queryStore.limit > 0) {
        ops.push({ type: 'limit', n: queryStore.limit });
      }

      return ops;
    }

    // Default: use query store operations
    return queryStore.operations;
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

    const operations = buildOperations();

    resultsStore.setLoading(true);

    const unsubscribeResult = connectionStore.onMessage(
      'queryResult',
      (message: Record<string, unknown>) => {
        resultsStore.setPivotResults(message);
        uiStore.onPivotResults();
        unsubscribeResult();
        unsubscribeError();
      },
    );

    const unsubscribeError = connectionStore.onMessage(
      'error',
      (message: Record<string, unknown>) => {
        resultsStore.setError((message['message'] as string) ?? 'Query failed');
        unsubscribeResult();
        unsubscribeError();
      },
    );

    connectionStore.send({
      type: 'query',
      datasetId: datasetStore.id,
      operations,
    });
  }

  // Legacy alias
  function execute(): void {
    executePivot();
  }

  /**
   * Check if query can be executed
   */
  function canExecute(): boolean {
    if (!connectionStore.isConnected || !datasetStore.hasData) {
      return false;
    }

    if (pivotStore.isConfigured) {
      return true;
    }

    return queryStore.isValid;
  }

  return {
    loadTableData,
    buildTableOperations,
    executePivot,
    execute,
    canExecute,
    buildOperations,
  };
}

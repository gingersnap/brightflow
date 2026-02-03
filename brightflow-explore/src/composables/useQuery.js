import { useConnectionStore } from '@/stores/connection'
import { useDatasetStore } from '@/stores/dataset'
import { useQueryStore } from '@/stores/query'
import { useResultsStore } from '@/stores/results'
import { usePivotStore } from '@/stores/pivot'
import { useUiStore } from '@/stores/ui'

/**
 * Query execution composable
 */
export function useQuery() {
  const connectionStore = useConnectionStore()
  const datasetStore = useDatasetStore()
  const queryStore = useQueryStore()
  const resultsStore = useResultsStore()
  const pivotStore = usePivotStore()
  const uiStore = useUiStore()

  /**
   * Build operations for table view (filters + limit)
   */
  function buildTableOperations() {
    const ops = []

    // Add filters
    if (queryStore.filters.length > 0) {
      queryStore.filters.forEach(filter => {
        if (filter.column && filter.op) {
          const op = {
            type: 'filter',
            column: filter.column,
            op: filter.op
          }
          if (!['isNull', 'isNotNull'].includes(filter.op)) {
            op.value = filter.value
          }
          ops.push(op)
        }
      })
    }

    // Add limit (0 means no limit)
    if (queryStore.limit > 0) {
      ops.push({ type: 'limit', n: queryStore.limit })
    }

    return ops
  }

  /**
   * Load table data with current filters and limit
   */
  function loadTableData() {
    if (!connectionStore.isConnected || !datasetStore.hasData) {
      return
    }

    const operations = buildTableOperations()

    resultsStore.setLoading(true)

    const unsubscribeResult = connectionStore.onMessage('queryResult', (message) => {
      resultsStore.setTableResults(message)
      unsubscribeResult()
      unsubscribeError()
    })

    const unsubscribeError = connectionStore.onMessage('error', (message) => {
      resultsStore.setError(message.message)
      unsubscribeResult()
      unsubscribeError()
    })

    connectionStore.send({
      type: 'query',
      datasetId: datasetStore.id,
      operations
    })
  }

  /**
   * Build operations based on current configuration
   */
  function buildOperations() {
    // If pivot is configured, use pivot operation (regardless of view mode)
    if (pivotStore.isConfigured) {
      const ops = []

      // Add any filters from query store
      if (queryStore.sections.filter.enabled && queryStore.filters.length > 0) {
        queryStore.filters.forEach(filter => {
          if (filter.column && filter.op) {
            const op = {
              type: 'filter',
              column: filter.column,
              op: filter.op
            }
            if (!['isNull', 'isNotNull'].includes(filter.op)) {
              op.value = filter.value
            }
            ops.push(op)
          }
        })
      }

      // Add pivot operation
      if (pivotStore.valueFields.length > 0) {
        const valueField = pivotStore.valueFields[0]
        const rowCols = pivotStore.rowFields.map(f => f.column)
        const colField = pivotStore.columnFields.length > 0 ? pivotStore.columnFields[0].column : null
        const aggFunc = valueField.aggregation || 'count'

        // Determine the best operation based on configuration
        // Note: The UI watcher should auto-add rows when values exist but rows/columns are empty
        // So rowCols.length === 0 && !colField should not happen in normal operation
        if (rowCols.length === 0 && !colField) {
          // Invalid state - should be handled by UI watcher, skip operation
          console.warn('[useQuery] Pivot has values but no rows/columns - waiting for UI to auto-add')
          return ops
        }

        if (!colField) {
          // Only rows, no column pivot - use groupBy
          ops.push({
            type: 'groupBy',
            by: rowCols,
            aggs: [{ column: valueField.column, function: aggFunc, alias: aggFunc }]
          })
        } else if (rowCols.length === 0) {
          // Only columns (no rows) - group by the column field
          // This shows one row per unique value in the column field
          ops.push({
            type: 'groupBy',
            by: [colField],
            aggs: [{ column: valueField.column, function: aggFunc, alias: aggFunc }]
          })
        } else {
          // Full pivot with both rows and columns
          ops.push({
            type: 'pivot',
            index: rowCols,
            columns: colField,
            values: valueField.column,
            agg: aggFunc
          })
        }

        console.log('[useQuery] Pivot/GroupBy operation:', ops[ops.length - 1])
      }

      // Add sort
      if (queryStore.sections.sort.enabled && queryStore.sortBy) {
        ops.push({
          type: 'sort',
          by: queryStore.sortBy,
          descending: queryStore.sortDescending
        })
      }

      // Add limit
      if (queryStore.sections.limit.enabled && queryStore.limit > 0) {
        ops.push({ type: 'limit', n: queryStore.limit })
      }

      return ops
    }

    // Default: use query store operations
    return queryStore.operations
  }

  /**
   * Execute pivot query (only when pivot is configured with values)
   */
  function executePivot() {
    if (!connectionStore.isConnected) {
      resultsStore.setError('Not connected to server')
      return
    }

    if (!datasetStore.hasData) {
      resultsStore.setError('No dataset loaded')
      return
    }

    if (!pivotStore.isConfigured) {
      // Don't execute if pivot isn't configured (needs values)
      return
    }

    const operations = buildOperations()

    resultsStore.setLoading(true)

    // Register one-time handler for query result
    const unsubscribeResult = connectionStore.onMessage('queryResult', (message) => {
      resultsStore.setPivotResults(message)

      // Auto-switch to pivot view on first pivot results
      uiStore.onPivotResults()

      unsubscribeResult()
      unsubscribeError()
    })

    // Handle errors
    const unsubscribeError = connectionStore.onMessage('error', (message) => {
      resultsStore.setError(message.message)
      unsubscribeResult()
      unsubscribeError()
    })

    // Send query
    connectionStore.send({
      type: 'query',
      datasetId: datasetStore.id,
      operations
    })
  }

  // Legacy alias
  function execute() {
    executePivot()
  }

  /**
   * Check if query can be executed
   */
  function canExecute() {
    if (!connectionStore.isConnected || !datasetStore.hasData) {
      return false
    }

    // If pivot is configured, it's executable
    if (pivotStore.isConfigured) {
      return true
    }

    return queryStore.isValid
  }

  return {
    loadTableData,
    buildTableOperations,
    executePivot,
    execute,
    canExecute,
    buildOperations
  }
}

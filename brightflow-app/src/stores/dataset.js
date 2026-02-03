import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { useConnectionStore } from './connection'
import { useUiStore } from './ui'
import { useResultsStore } from './results'
import { useQueryStore } from './query'

export const useDatasetStore = defineStore('dataset', () => {
  // State
  const id = ref('default')
  const name = ref(null)
  const rowCount = ref(null)
  const columnCount = ref(null)
  const columns = ref([])
  const loading = ref(false)
  const error = ref(null)

  // Computed
  const numericColumns = computed(() =>
    columns.value.filter(c => ['int', 'float', 'decimal', 'number'].includes(c.dtype))
  )

  const stringColumns = computed(() =>
    columns.value.filter(c => ['string', 'text', 'varchar'].includes(c.dtype))
  )

  const hasData = computed(() => columns.value.length > 0)

  // Actions
  function fetchMetadata(datasetId = 'default') {
    const connectionStore = useConnectionStore()

    if (!connectionStore.isConnected) {
      error.value = 'Not connected'
      loading.value = false
      return
    }

    loading.value = true
    error.value = null

    // Timeout after 10 seconds
    const timeout = setTimeout(() => {
      if (loading.value) {
        loading.value = false
        error.value = 'Request timed out'
        unsubscribe()
        unsubscribeError()
      }
    }, 10000)

    // Register one-time handler for metadata response
    const unsubscribe = connectionStore.onMessage('metadata', (message) => {
      // Backend uses snake_case: dataset_id, row_count
      if (message.dataset_id === datasetId) {
        clearTimeout(timeout)
        id.value = message.dataset_id
        name.value = message.name
        rowCount.value = message.row_count
        columnCount.value = message.columns?.length || 0
        columns.value = message.columns || []
        loading.value = false
        unsubscribe()
        unsubscribeError()

        // Reset UI state for new dataset
        const uiStore = useUiStore()
        uiStore.resetForNewDataset()

        // Load raw table data
        loadInitialData()
      }
    })

    // Handle errors
    const unsubscribeError = connectionStore.onMessage('error', (message) => {
      clearTimeout(timeout)
      error.value = message.message
      loading.value = false
      unsubscribe()
      unsubscribeError()
    })

    // Send request
    connectionStore.send({
      type: 'getMetadata',
      datasetId
    })
  }

  function getColumnByName(columnName) {
    return columns.value.find(c => c.name === columnName)
  }

  function getColumnType(columnName) {
    const column = getColumnByName(columnName)
    return column?.dtype || 'string'
  }

  function reset() {
    id.value = 'default'
    name.value = null
    rowCount.value = null
    columnCount.value = null
    columns.value = []
    error.value = null
  }

  // Load initial table data after metadata is received
  function loadInitialData() {
    const connectionStore = useConnectionStore()
    const resultsStore = useResultsStore()
    const queryStore = useQueryStore()

    if (!connectionStore.isConnected || columns.value.length === 0) {
      return
    }

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

    // Query with limit from query store (default 100)
    const ops = []
    if (queryStore.limit > 0) {
      ops.push({ type: 'limit', n: queryStore.limit })
    }

    connectionStore.send({
      type: 'query',
      datasetId: id.value,
      operations: ops
    })
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
    reset
  }
})

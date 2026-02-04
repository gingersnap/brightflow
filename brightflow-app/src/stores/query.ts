import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { Filter, Aggregation, QuerySections, PivotState, QueryOperation } from '@/types'

type SectionKey = keyof QuerySections

export const useQueryStore = defineStore('query', () => {
  // Section states
  const sections = ref<QuerySections>({
    filter: { enabled: true, collapsed: false },
    select: { enabled: false, collapsed: true },
    groupBy: { enabled: false, collapsed: true },
    pivot: { enabled: false, collapsed: true },
    sort: { enabled: false, collapsed: true },
    limit: { enabled: true, collapsed: false }
  })

  // Filter state
  const filters = ref<Filter[]>([])

  // Select state
  const selectedColumns = ref<string[]>([])

  // Group by state
  const groupByColumns = ref<string[]>([])
  const aggregations = ref<Aggregation[]>([])

  // Pivot state
  const pivot = ref<PivotState>({
    index: [],
    columns: null,
    values: null,
    agg: 'count'
  })

  // Sort state
  const sortBy = ref<string | null>(null)
  const sortDescending = ref(false)

  // Limit state (default 100 for table view)
  const limit = ref(100)

  // Computed: Build operations array for API
  const operations = computed((): QueryOperation[] => {
    const ops: QueryOperation[] = []

    // Add filters
    if (sections.value.filter.enabled && filters.value.length > 0) {
      filters.value.forEach(filter => {
        if (filter.column && filter.op) {
          const op: QueryOperation = {
            type: 'filter',
            column: filter.column,
            op: filter.op
          }
          // Only add value if operator requires it
          if (!['isNull', 'isNotNull'].includes(filter.op)) {
            op.value = filter.value
          }
          ops.push(op)
        }
      })
    }

    // Add group by
    if (sections.value.groupBy.enabled && groupByColumns.value.length > 0) {
      ops.push({
        type: 'groupBy',
        by: groupByColumns.value,
        aggs: aggregations.value.map(agg => ({
          column: agg.column,
          function: agg.function,
          alias: agg.alias || `${agg.function}_${agg.column}`
        }))
      })
    }

    // Add pivot
    if (sections.value.pivot.enabled && pivot.value.values) {
      ops.push({
        type: 'pivot',
        index: pivot.value.index,
        columns: pivot.value.columns,
        values: pivot.value.values,
        agg: pivot.value.agg
      })
    }

    // Add select
    if (sections.value.select.enabled && selectedColumns.value.length > 0) {
      ops.push({
        type: 'select',
        columns: selectedColumns.value
      })
    }

    // Add sort
    if (sections.value.sort.enabled && sortBy.value) {
      ops.push({
        type: 'sort',
        by: sortBy.value,
        descending: sortDescending.value
      })
    }

    // Add limit
    if (sections.value.limit.enabled && limit.value > 0) {
      ops.push({
        type: 'limit',
        n: limit.value
      })
    }

    return ops
  })

  // Computed: Preview texts for each section
  const previewTexts = computed(() => ({
    filter: filters.value.length > 0
      ? `${filters.value.length} filter${filters.value.length > 1 ? 's' : ''}`
      : 'No filters',
    select: selectedColumns.value.length > 0
      ? `${selectedColumns.value.length} columns`
      : 'All columns',
    groupBy: groupByColumns.value.length > 0
      ? `By: ${groupByColumns.value.join(', ')}`
      : 'Not grouped',
    pivot: pivot.value.values
      ? `${pivot.value.index.length} rows, ${pivot.value.columns ?? 'no'} columns`
      : 'Not configured',
    sort: sortBy.value
      ? `${sortBy.value} ${sortDescending.value ? 'DESC' : 'ASC'}`
      : 'Not sorted',
    limit: `${limit.value.toLocaleString()} rows`
  }))

  const isValid = computed(() => {
    // Basic validation - at minimum we need a valid configuration
    return true
  })

  // Actions
  function toggleSection(section: SectionKey): void {
    sections.value[section].enabled = !sections.value[section].enabled
  }

  function toggleCollapse(section: SectionKey): void {
    sections.value[section].collapsed = !sections.value[section].collapsed
  }

  function addFilter(): void {
    filters.value.push({
      id: crypto.randomUUID(),
      column: null,
      op: 'eq',
      value: null
    })
  }

  function updateFilter(id: string, updates: Partial<Filter>): void {
    const filter = filters.value.find(f => f.id === id)
    if (filter) {
      Object.assign(filter, updates)
    }
  }

  function removeFilter(id: string): void {
    filters.value = filters.value.filter(f => f.id !== id)
  }

  function addAggregation(): void {
    aggregations.value.push({
      id: crypto.randomUUID(),
      column: '*',
      function: 'count',
      alias: ''
    })
  }

  function updateAggregation(id: string, updates: Partial<Aggregation>): void {
    const agg = aggregations.value.find(a => a.id === id)
    if (agg) {
      Object.assign(agg, updates)
    }
  }

  function removeAggregation(id: string): void {
    aggregations.value = aggregations.value.filter(a => a.id !== id)
  }

  function reset(): void {
    filters.value = []
    selectedColumns.value = []
    groupByColumns.value = []
    aggregations.value = []
    pivot.value = { index: [], columns: null, values: null, agg: 'count' }
    sortBy.value = null
    sortDescending.value = false
    limit.value = 100

    // Reset section states
    sections.value = {
      filter: { enabled: true, collapsed: false },
      select: { enabled: false, collapsed: true },
      groupBy: { enabled: false, collapsed: true },
      pivot: { enabled: false, collapsed: true },
      sort: { enabled: false, collapsed: true },
      limit: { enabled: true, collapsed: false }
    }
  }

  return {
    // State
    sections,
    filters,
    selectedColumns,
    groupByColumns,
    aggregations,
    pivot,
    sortBy,
    sortDescending,
    limit,
    // Computed
    operations,
    previewTexts,
    isValid,
    // Actions
    toggleSection,
    toggleCollapse,
    addFilter,
    updateFilter,
    removeFilter,
    addAggregation,
    updateAggregation,
    removeAggregation,
    reset
  }
})

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export const usePivotStore = defineStore('pivot', () => {
  // === Bucket State ===
  // Rows bucket - columns that become row headers
  const rowFields = ref([])

  // Columns bucket - column that becomes column headers
  const columnFields = ref([])

  // Values bucket - columns with aggregations
  const valueFields = ref([])

  // === Settings ===
  const showSubtotals = ref(true)
  const showRowTotals = ref(true)
  const showColumnTotals = ref(true)
  const showConditionalFormatting = ref(false)
  const decimalPlaces = ref(2) // Number of decimal places for numeric values

  // === UI State ===
  // Track which row groups are collapsed
  const collapsedGroups = ref(new Set())

  // Conditional formatting rules
  const formatRules = ref([])

  // === Computed ===

  // Check if pivot is configured (has at least values)
  const isConfigured = computed(() => {
    return valueFields.value.length > 0
  })

  // Check if pivot has row grouping
  const hasRowGroups = computed(() => {
    return rowFields.value.length > 0
  })

  // Check if pivot has column breakdown
  const hasColumnBreakdown = computed(() => {
    return columnFields.value.length > 0
  })

  // Build operations for API
  const pivotOperation = computed(() => {
    if (!isConfigured.value) return null

    // For multi-value pivot, we need an enhanced format
    const values = valueFields.value.map(v => ({
      column: v.column,
      agg: v.aggregation
    }))

    return {
      type: 'pivot',
      index: rowFields.value.map(f => f.column),
      columns: columnFields.value.length > 0 ? columnFields.value[0].column : null,
      values: values.length === 1 ? values[0].column : values,
      agg: values.length === 1 ? values[0].agg : values.map(v => v.agg),
      includeSubtotals: showSubtotals.value,
      includeTotals: showRowTotals.value || showColumnTotals.value
    }
  })

  // === Actions ===

  function addRowField(column, dtype) {
    // Check if already added
    if (rowFields.value.some(f => f.column === column)) return

    rowFields.value.push({
      id: crypto.randomUUID(),
      column,
      dtype
    })
  }

  function removeRowField(id) {
    rowFields.value = rowFields.value.filter(f => f.id !== id)
  }

  function reorderRowFields(newOrder) {
    rowFields.value = newOrder
  }

  function addColumnField(column, dtype) {
    // Only allow one column field (Metabase behavior)
    columnFields.value = [{
      id: crypto.randomUUID(),
      column,
      dtype
    }]
  }

  function removeColumnField(id) {
    columnFields.value = columnFields.value.filter(f => f.id !== id)
  }

  function addValueField(column, dtype, aggregation = null) {
    // Choose default aggregation based on type
    // Numeric types default to sum, strings default to count
    const isNumeric = ['int', 'float', 'decimal', 'number', 'i64', 'f64'].includes(dtype)
    const defaultAgg = isNumeric ? 'sum' : 'count'

    valueFields.value.push({
      id: crypto.randomUUID(),
      column,
      dtype,
      aggregation: aggregation || defaultAgg
    })
  }

  function updateValueField(id, updates) {
    const field = valueFields.value.find(f => f.id === id)
    if (field) {
      Object.assign(field, updates)
    }
  }

  function removeValueField(id) {
    valueFields.value = valueFields.value.filter(f => f.id !== id)
  }

  function reorderValueFields(newOrder) {
    valueFields.value = newOrder
  }

  // Group collapse management
  function toggleGroup(groupKey) {
    if (collapsedGroups.value.has(groupKey)) {
      collapsedGroups.value.delete(groupKey)
    } else {
      collapsedGroups.value.add(groupKey)
    }
    // Trigger reactivity
    collapsedGroups.value = new Set(collapsedGroups.value)
  }

  function isGroupCollapsed(groupKey) {
    return collapsedGroups.value.has(groupKey)
  }

  function expandAllGroups() {
    collapsedGroups.value = new Set()
  }

  function collapseAllGroups(groupKeys) {
    collapsedGroups.value = new Set(groupKeys)
  }

  // Conditional formatting
  function addFormatRule(rule) {
    formatRules.value.push({
      id: crypto.randomUUID(),
      ...rule
    })
  }

  function removeFormatRule(id) {
    formatRules.value = formatRules.value.filter(r => r.id !== id)
  }

  // Reset all pivot state
  function reset() {
    rowFields.value = []
    columnFields.value = []
    valueFields.value = []
    showSubtotals.value = true
    showRowTotals.value = true
    showColumnTotals.value = true
    showConditionalFormatting.value = false
    decimalPlaces.value = 2
    collapsedGroups.value = new Set()
    formatRules.value = []
  }

  // Flip/swap rows and columns
  function flipRowsAndColumns() {
    const oldRows = [...rowFields.value]
    const oldColumns = [...columnFields.value]

    // Columns bucket only allows one item, so take first row if multiple
    if (oldRows.length > 0) {
      columnFields.value = [oldRows[0]]
      // Remaining rows stay as rows
      rowFields.value = oldRows.slice(1)
    } else {
      columnFields.value = []
      rowFields.value = []
    }

    // Move old column to rows
    if (oldColumns.length > 0) {
      rowFields.value = [...oldColumns, ...rowFields.value]
    }

    // Clear collapsed groups since structure changed
    collapsedGroups.value = new Set()
  }

  // Move field between buckets
  function moveField(fieldId, fromBucket, toBucket) {
    let field = null

    // Find and remove from source bucket
    if (fromBucket === 'rows') {
      const idx = rowFields.value.findIndex(f => f.id === fieldId)
      if (idx !== -1) {
        field = rowFields.value.splice(idx, 1)[0]
      }
    } else if (fromBucket === 'columns') {
      const idx = columnFields.value.findIndex(f => f.id === fieldId)
      if (idx !== -1) {
        field = columnFields.value.splice(idx, 1)[0]
      }
    } else if (fromBucket === 'values') {
      const idx = valueFields.value.findIndex(f => f.id === fieldId)
      if (idx !== -1) {
        field = valueFields.value.splice(idx, 1)[0]
      }
    }

    if (!field) return

    // Add to destination bucket
    if (toBucket === 'rows') {
      addRowField(field.column, field.dtype)
    } else if (toBucket === 'columns') {
      addColumnField(field.column, field.dtype)
    } else if (toBucket === 'values') {
      addValueField(field.column, field.dtype, field.aggregation)
    }
  }

  return {
    // State
    rowFields,
    columnFields,
    valueFields,
    showSubtotals,
    showRowTotals,
    showColumnTotals,
    showConditionalFormatting,
    decimalPlaces,
    collapsedGroups,
    formatRules,

    // Computed
    isConfigured,
    hasRowGroups,
    hasColumnBreakdown,
    pivotOperation,

    // Actions
    addRowField,
    removeRowField,
    reorderRowFields,
    addColumnField,
    removeColumnField,
    addValueField,
    updateValueField,
    removeValueField,
    reorderValueFields,
    toggleGroup,
    isGroupCollapsed,
    expandAllGroups,
    collapseAllGroups,
    addFormatRule,
    removeFormatRule,
    moveField,
    flipRowsAndColumns,
    reset
  }
})

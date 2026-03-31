import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import type { AggFn, PivotField, PivotOperation } from '@/types';

type BucketName = 'rows' | 'columns' | 'values';

export const usePivotStore = defineStore('pivot', () => {
  // === Bucket State ===
  // Rows bucket - columns that become row headers
  const rowFields = ref<PivotField[]>([]);

  // Columns bucket - column that becomes column headers
  const columnFields = ref<PivotField[]>([]);

  // Values bucket - columns with aggregations
  const valueFields = ref<PivotField[]>([]);

  // === Settings ===
  const showSubtotals = ref(true);
  const showRowTotals = ref(true);
  const showColumnTotals = ref(true);
  const showConditionalFormatting = ref(false);
  const decimalPlaces = ref(2); // Number of decimal places for numeric values

  // === UI State ===
  // Track which row groups are collapsed
  const collapsedGroups = ref<Set<string>>(new Set());

  // === Computed ===

  // Check if pivot is configured (has at least values)
  const isConfigured = computed(() => valueFields.value.length > 0);

  // Check if pivot has row grouping
  const hasRowGroups = computed(() => rowFields.value.length > 0);

  // Check if pivot has column breakdown
  const hasColumnBreakdown = computed(() => columnFields.value.length > 0);

  // Build operations for API
  const pivotOperation = computed((): PivotOperation | null => {
    if (!isConfigured.value) {
      return null;
    }

    // For multi-value pivot, we need an enhanced format
    const values = valueFields.value.map((v) => ({
      agg: v.aggregation ?? 'count',
      column: v.column,
    }));

    const firstValue = values[0];
    if (!firstValue) {
      return null;
    }

    return {
      agg: values.length === 1 ? firstValue.agg : values.map((v) => v.agg),
      columns: columnFields.value.length > 0 ? (columnFields.value[0]?.column ?? null) : null,
      includeSubtotals: showSubtotals.value,
      includeTotals: showRowTotals.value || showColumnTotals.value,
      index: rowFields.value.map((f) => f.column),
      type: 'pivot',
      values: values.length === 1 ? firstValue.column : values,
    };
  });

  // === Actions ===

  function addRowField(column: string, dtype: string): void {
    // Check if already added
    if (rowFields.value.some((f) => f.column === column)) {
      return;
    }

    rowFields.value.push({
      column,
      dtype,
      id: crypto.randomUUID(),
    });
  }

  function removeRowField(id: string): void {
    rowFields.value = rowFields.value.filter((f) => f.id !== id);
  }

  function reorderRowFields(newOrder: PivotField[]): void {
    rowFields.value = newOrder;
  }

  function addColumnField(column: string, dtype: string): void {
    // Only allow one column field (Metabase behavior)
    columnFields.value = [
      {
        column,
        dtype,
        id: crypto.randomUUID(),
      },
    ];
  }

  function removeColumnField(id: string): void {
    columnFields.value = columnFields.value.filter((f) => f.id !== id);
  }

  function addValueField(column: string, dtype: string, aggregation: AggFn | null = null): void {
    // Choose default aggregation based on type
    // Numeric types default to sum, strings default to count
    const isNumeric = ['int', 'float', 'decimal', 'number', 'i64', 'f64'].includes(dtype);
    const defaultAgg: AggFn = isNumeric ? 'sum' : 'count';

    valueFields.value.push({
      aggregation: aggregation ?? defaultAgg,
      column,
      dtype,
      id: crypto.randomUUID(),
    });
  }

  function updateValueField(id: string, updates: Partial<PivotField>): void {
    const field = valueFields.value.find((f) => f.id === id);
    if (field) {
      Object.assign(field, updates);
    }
  }

  function removeValueField(id: string): void {
    valueFields.value = valueFields.value.filter((f) => f.id !== id);
  }

  function reorderValueFields(newOrder: PivotField[]): void {
    valueFields.value = newOrder;
  }

  // Group collapse management
  function toggleGroup(groupKey: string): void {
    if (collapsedGroups.value.has(groupKey)) {
      collapsedGroups.value.delete(groupKey);
    } else {
      collapsedGroups.value.add(groupKey);
    }
    // Trigger reactivity
    collapsedGroups.value = new Set(collapsedGroups.value);
  }

  function isGroupCollapsed(groupKey: string): boolean {
    return collapsedGroups.value.has(groupKey);
  }

  function expandAllGroups(): void {
    collapsedGroups.value = new Set();
  }

  function collapseAllGroups(groupKeys: string[]): void {
    collapsedGroups.value = new Set(groupKeys);
  }

  // Reset all pivot state
  function reset(): void {
    rowFields.value = [];
    columnFields.value = [];
    valueFields.value = [];
    showSubtotals.value = true;
    showRowTotals.value = true;
    showColumnTotals.value = true;
    showConditionalFormatting.value = false;
    decimalPlaces.value = 2;
    collapsedGroups.value = new Set();
  }

  // Flip/swap rows and columns
  function flipRowsAndColumns(): void {
    const oldRows = [...rowFields.value];
    const oldColumns = [...columnFields.value];

    // Columns bucket only allows one item, so take first row if multiple
    if (oldRows.length > 0) {
      const firstRow = oldRows[0];
      if (firstRow) {
        columnFields.value = [firstRow];
      }
      // Remaining rows stay as rows
      rowFields.value = oldRows.slice(1);
    } else {
      columnFields.value = [];
      rowFields.value = [];
    }

    // Move old column to rows
    if (oldColumns.length > 0) {
      rowFields.value = [...oldColumns, ...rowFields.value];
    }

    // Clear collapsed groups since structure changed
    collapsedGroups.value = new Set();
  }

  // Move field between buckets
  function moveField(fieldId: string, fromBucket: BucketName, toBucket: BucketName): void {
    let field: PivotField | null = null;

    // Find and remove from source bucket
    if (fromBucket === 'rows') {
      const idx = rowFields.value.findIndex((f) => f.id === fieldId);
      if (idx !== -1) {
        const removed = rowFields.value.splice(idx, 1)[0];
        if (removed) {
          field = removed;
        }
      }
    } else if (fromBucket === 'columns') {
      const idx = columnFields.value.findIndex((f) => f.id === fieldId);
      if (idx !== -1) {
        const removed = columnFields.value.splice(idx, 1)[0];
        if (removed) {
          field = removed;
        }
      }
    } else if (fromBucket === 'values') {
      const idx = valueFields.value.findIndex((f) => f.id === fieldId);
      if (idx !== -1) {
        const removed = valueFields.value.splice(idx, 1)[0];
        if (removed) {
          field = removed;
        }
      }
    }

    if (!field) {
      return;
    }

    // Add to destination bucket
    if (toBucket === 'rows') {
      addRowField(field.column, field.dtype);
    } else if (toBucket === 'columns') {
      addColumnField(field.column, field.dtype);
    } else if (toBucket === 'values') {
      addValueField(field.column, field.dtype, field.aggregation);
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
    moveField,
    flipRowsAndColumns,
    reset,
  };
});

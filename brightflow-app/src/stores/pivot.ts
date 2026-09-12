/**
 * Pivot builder state: the row, column, and value buckets.
 *
 * Field order within a bucket is meaningful (it is the nesting order of the
 * resulting headers), which is why buckets are arrays rather than sets. Row
 * and column fields carry their own sort, the way Excel's do; a new field
 * starts largest-first, and the table and chart both order by it.
 *
 * A dropped column brings its stored role along: a value field's default
 * aggregation follows the role (`sum` for a measure, `count` for anything
 * else), and only falls back to the dtype rule when the column has no role.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import type { AggFn, PivotField } from '@/types';
import type { ColumnRole } from '@/types/generated';
import { isNumericDtype } from '@/utils/dtype';
import { DEFAULT_FIELD_SORT, type FieldSort } from '@/utils/pivotOrder';

/** What a column dropped into a bucket carries with it. */
export interface DroppedColumn {
  column: string;
  dtype: string;
  role?: ColumnRole | null;
}

/** Role first, dtype second: a numeric column the engine calls a dimension counts. */
export function defaultAggregation(field: DroppedColumn): AggFn {
  if (field.role != null) {
    return field.role === 'measure' ? 'sum' : 'count';
  }
  return isNumericDtype(field.dtype) ? 'sum' : 'count';
}

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
  const showColumnTotals = ref(true);
  const showConditionalFormatting = ref(false);
  const decimalPlaces = ref(2); // Number of decimal places for numeric values

  // === UI State ===
  // Track which row groups are collapsed
  const collapsedGroups = ref<Set<string>>(new Set());

  // === Computed ===

  // Check if pivot is configured (has at least values)
  const isConfigured = computed(() => valueFields.value.length > 0);

  // === Actions ===

  function addRowField(field: DroppedColumn): void {
    // Check if already added
    if (rowFields.value.some((f) => f.column === field.column)) {
      return;
    }

    rowFields.value.push({
      column: field.column,
      dtype: field.dtype,
      id: crypto.randomUUID(),
      role: field.role ?? null,
      sort: { ...DEFAULT_FIELD_SORT },
    });
  }

  /** Change a row or column field's display order. */
  function setFieldSort(id: string, sort: FieldSort): void {
    const field = [...rowFields.value, ...columnFields.value].find((f) => f.id === id);
    if (field) {
      field.sort = { ...sort };
    }
  }

  function removeRowField(id: string): void {
    rowFields.value = rowFields.value.filter((f) => f.id !== id);
  }

  function reorderRowFields(newOrder: PivotField[]): void {
    rowFields.value = newOrder;
  }

  function addColumnField(field: DroppedColumn): void {
    // Only allow one column field (Metabase behavior)
    columnFields.value = [
      {
        column: field.column,
        dtype: field.dtype,
        id: crypto.randomUUID(),
        role: field.role ?? null,
        sort: { ...DEFAULT_FIELD_SORT },
      },
    ];
  }

  function removeColumnField(id: string): void {
    columnFields.value = columnFields.value.filter((f) => f.id !== id);
  }

  function addValueField(field: DroppedColumn, aggregation: AggFn | null = null): void {
    valueFields.value.push({
      aggregation: aggregation ?? defaultAggregation(field),
      column: field.column,
      dtype: field.dtype,
      id: crypto.randomUUID(),
      role: field.role ?? null,
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

  return {
    // State
    rowFields,
    columnFields,
    valueFields,
    showSubtotals,
    showColumnTotals,
    showConditionalFormatting,
    decimalPlaces,

    // Computed
    isConfigured,

    // Actions
    setFieldSort,
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
    flipRowsAndColumns,
    reset,
  };
});

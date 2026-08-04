<script setup lang="ts">
/**
 * Renders backend-pivoted results as a hand-rolled table — the domain grid
 * (collapsible group-header rows, subtotals, column totals, per-cell heatmap
 * shading) is the justified raw-markup exception to Nuxt-UI-first. With
 * multiple row fields, rows are grouped client-side by the first index
 * column, with subtotals accumulated per group.
 */

import { ChevronDown, ChevronRight } from '@lucide/vue';
import { computed } from 'vue';

import { usePivotStore } from '@/stores/pivot';
import { useResultsStore } from '@/stores/results';
import { isFloatDtype, isNumericDtype } from '@/utils/dtype';

const pivotStore = usePivotStore();
const resultsStore = useResultsStore();

interface ColumnInfo {
  name: string | undefined;
  dtype: string | undefined;
}

interface PivotRow {
  id: string;
  indexValues: unknown[];
  dataValues: unknown[];
}

interface PivotData {
  indexColumns: ColumnInfo[];
  valueColumns: ColumnInfo[];
  rows: PivotRow[];
}

// Process results - the backend already returns pivoted data
// We just need to display it with proper formatting
const pivotData = computed((): PivotData | null => {
  if (!resultsStore.hasPivotResults) {
    return null;
  }

  const columns = resultsStore.pivotColumns;
  const rows = resultsStore.pivotRows;
  const colNames = columns.map((c) => c.name);
  const colTypes = columns.map((c) => c.dtype);

  // The first N columns are the index (row labels)
  // The remaining columns are the pivoted values
  const indexCols = new Set(pivotStore.rowFields.map((f) => f.column));

  // Determine which columns are index vs values
  const indexColIndices: number[] = [];
  const valueColIndices: number[] = [];

  colNames.forEach((name, idx) => {
    if (indexCols.has(name)) {
      indexColIndices.push(idx);
    } else {
      valueColIndices.push(idx);
    }
  });

  // If no index columns found in results, treat first column as index
  if (indexColIndices.length === 0 && colNames.length > 0) {
    indexColIndices.push(0);
    valueColIndices.length = 0;
    for (let i = 1; i < colNames.length; i++) {
      valueColIndices.push(i);
    }
  }

  return {
    indexColumns: indexColIndices.map((i) => ({ name: colNames[i], dtype: colTypes[i] })),
    rows: rows.map((row, rowIdx) => ({
      id: `row-${rowIdx}`,
      indexValues: indexColIndices.map((i) => row[i]),
      dataValues: valueColIndices.map((i) => row[i]),
    })),
    valueColumns: valueColIndices.map((i) => ({ name: colNames[i], dtype: colTypes[i] })),
  };
});

interface GroupedRow {
  id: string;
  level: number;
  isGroup: boolean;
  groupKey: string | null;
  groupLabel?: unknown;
  rowCount?: number;
  indexValues: unknown[];
  dataValues: unknown[];
  isCollapsed?: boolean;
  displayIndexValues?: unknown[];
}

interface Group {
  key: unknown;
  rows: PivotRow[];
  subtotals: number[];
}

// Group rows hierarchically when there are multiple row fields
const groupedRows = computed((): GroupedRow[] => {
  if (!pivotData.value) {
    return [];
  }

  const numIndexCols = pivotData.value.indexColumns.length;

  // If only one index column, no grouping needed
  if (numIndexCols <= 1) {
    return pivotData.value.rows.map((row) => ({
      ...row,
      groupKey: null,
      isGroup: false,
      level: 0,
    }));
  }

  // Group by first index column(s)
  const result: GroupedRow[] = [];
  const groups = new Map<unknown, Group>();

  // Build groups
  const pData = pivotData.value;
  pData.rows.forEach((row) => {
    const groupKeyVal = row.indexValues[0];
    if (!groups.has(groupKeyVal)) {
      groups.set(groupKeyVal, {
        key: groupKeyVal,
        rows: [],
        subtotals: pData.valueColumns.map(() => 0),
      });
    }
    const group = groups.get(groupKeyVal);
    if (group) {
      group.rows.push(row);

      // Accumulate subtotals
      row.dataValues.forEach((val, idx) => {
        if (typeof val === 'number' && group.subtotals[idx] !== undefined) {
          group.subtotals[idx] += val;
        }
      });
    }
  });

  // Flatten into displayable rows with group headers
  groups.forEach((group, key) => {
    const groupKey = `group-${key}`;
    const isCollapsed = pivotStore.isGroupCollapsed(groupKey);

    // Add group header row
    result.push({
      dataValues: pivotStore.showSubtotals ? group.subtotals : [],
      groupKey,
      groupLabel: key,
      id: groupKey,
      indexValues: [key],
      isCollapsed,
      isGroup: true,
      level: 0,
      rowCount: group.rows.length,
    });

    // Add child rows if not collapsed
    if (!isCollapsed) {
      group.rows.forEach((row) => {
        result.push({
          ...row,
          level: 1,
          isGroup: false,
          groupKey: null,
          // Hide first index value (shown in group header)
          displayIndexValues: row.indexValues.slice(1),
        });
      });
    }
  });

  return result;
});

// Check if we have hierarchical grouping
const hasGrouping = computed(() => pivotData.value && pivotData.value.indexColumns.length > 1);

// Get all group keys for expand/collapse all
const allGroupKeys = computed((): string[] => {
  if (!hasGrouping.value) {
    return [];
  }
  return groupedRows.value
    .filter((row) => row.isGroup && row.groupKey !== null)
    .map((row) => row.groupKey as string);
});

// Expand all groups
function expandAll() {
  pivotStore.expandAllGroups();
}

// Collapse all groups
function collapseAll() {
  pivotStore.collapseAllGroups(allGroupKeys.value);
}

interface ColumnStat {
  min: number;
  max: number;
  range: number;
}

// Calculate min/max for each value column (for conditional formatting)
const columnStats = computed((): (ColumnStat | null)[] => {
  if (!pivotData.value) {
    return [];
  }

  const pData = pivotData.value;
  return pData.valueColumns.map((_col, colIdx) => {
    let min = Infinity;
    let max = -Infinity;
    let hasValues = false;

    pData.rows.forEach((row) => {
      const val = row.dataValues[colIdx];
      if (typeof val === 'number' && !isNaN(val)) {
        min = Math.min(min, val);
        max = Math.max(max, val);
        hasValues = true;
      }
    });

    return hasValues ? { max, min, range: max - min } : null;
  });
});

// Calculate column totals
const columnTotals = computed((): (number | null)[] | null => {
  if (!pivotData.value || !pivotStore.showColumnTotals) {
    return null;
  }

  const pData = pivotData.value;
  const totals = pData.valueColumns.map((_col, colIdx) => {
    let sum = 0;
    let count = 0;
    pData.rows.forEach((row) => {
      const val = row.dataValues[colIdx];
      if (typeof val === 'number') {
        sum += val;
        count++;
      }
    });
    return count > 0 ? sum : null;
  });

  return totals;
});

// Get cell background color based on value (conditional formatting)
function getCellStyle(value: unknown, colIdx: number): Record<string, string> {
  if (!pivotStore.showConditionalFormatting) {
    return {};
  }

  const stats = columnStats.value[colIdx];
  if (!stats || typeof value !== 'number' || stats.range === 0) {
    return {};
  }

  // Calculate position in range (0 to 1)
  const position = (value - stats.min) / stats.range;

  // Color scale: light blue (low) to dark blue (high)
  // Using HSL for smooth gradients
  const hue = 210; // Blue
  const saturation = 70;
  const lightness = 95 - position * 40; // 95% (light) to 55% (darker)

  return {
    backgroundColor: `hsl(${hue}, ${saturation}%, ${lightness}%)`,
  };
}

// Format cell value
function formatValue(value: unknown, dtype: string | undefined): string {
  if (value === null || value === undefined) {
    return '—';
  }
  if (typeof value === 'number') {
    const decimals = pivotStore.decimalPlaces;
    // Format based on dtype
    if (isFloatDtype(dtype)) {
      return value.toLocaleString(undefined, {
        maximumFractionDigits: decimals,
        minimumFractionDigits: 0,
      });
    }
    // Integers - no decimals unless value has them
    if (Number.isInteger(value)) {
      return value.toLocaleString();
    }
    // Non-integer without explicit float type
    return value.toLocaleString(undefined, {
      maximumFractionDigits: decimals,
      minimumFractionDigits: 0,
    });
  }
  return String(value);
}

// Check if a value is numeric
function isNumeric(dtype: string | undefined): boolean {
  if (!dtype) {
    return false;
  }
  return isNumericDtype(dtype);
}
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Empty state if no data -->
    <div v-if="!pivotData" class="flex h-full items-center justify-center text-muted">
      <div class="text-center">
        <p class="text-sm">No pivot data</p>
        <p class="mt-1 text-sm text-muted/70">Configure your pivot and run the query</p>
      </div>
    </div>

    <!-- Grouping toolbar -->
    <div
      v-if="hasGrouping && pivotData"
      class="flex items-center gap-2 border-b border-default bg-muted/30 px-3 py-1.5 text-sm"
    >
      <span class="text-muted">Groups:</span>
      <UButton variant="ghost" color="neutral" size="md" @click="expandAll">Expand all</UButton>
      <UButton variant="ghost" color="neutral" size="md" @click="collapseAll">
        Collapse all
      </UButton>
    </div>

    <!-- Pivot Table -->
    <div v-if="pivotData" class="flex-1 overflow-auto">
      <table class="w-full border-collapse text-sm">
        <!-- Header -->
        <thead class="sticky top-0 z-1 bg-muted/50 backdrop-blur">
          <tr>
            <!-- Index column headers -->
            <th
              v-for="col in pivotData.indexColumns"
              :key="'idx-' + col.name"
              class="border-b border-default bg-muted/50 px-3 py-2 text-left text-xs font-semibold tracking-wide text-muted uppercase"
            >
              {{ col.name }}
            </th>

            <!-- Value column headers -->
            <th
              v-for="col in pivotData.valueColumns"
              :key="'val-' + col.name"
              class="border-b border-default bg-muted/50 px-3 py-2 text-right text-xs font-semibold tracking-wide text-muted uppercase"
            >
              {{ col.name }}
            </th>
          </tr>
        </thead>

        <!-- Body -->
        <tbody>
          <template v-for="row in groupedRows" :key="row.id">
            <!-- Group header row -->
            <tr
              v-if="row.isGroup && row.groupKey"
              class="cursor-pointer bg-muted/40 transition-colors hover:bg-muted/60"
              @click="pivotStore.toggleGroup(row.groupKey)"
            >
              <!-- Group label with expand/collapse icon -->
              <td
                :colspan="hasGrouping ? pivotData.indexColumns.length : 1"
                class="border-b border-default px-3 py-2 font-semibold"
              >
                <div class="flex items-center gap-2">
                  <component
                    :is="row.isCollapsed ? ChevronRight : ChevronDown"
                    class="h-4 w-4 text-muted"
                  />
                  <span>{{ row.groupLabel }}</span>
                  <span class="text-xs font-normal text-muted">({{ row.rowCount }})</span>
                </div>
              </td>

              <!-- Subtotal values for group -->
              <td
                v-for="(value, idx) in row.dataValues"
                :key="'subtotal-' + idx"
                class="border-b border-default px-3 py-2 text-right font-semibold tabular-nums"
                :class="{
                  'font-mono': isNumeric(pivotData.valueColumns[idx]?.dtype),
                }"
              >
                {{ formatValue(value, pivotData.valueColumns[idx]?.dtype) }}
              </td>
              <!-- Empty cells if subtotals disabled -->
              <td
                v-if="row.dataValues.length === 0"
                v-for="idx in pivotData.valueColumns.length"
                :key="'empty-' + idx"
                class="border-b border-default px-3 py-2"
              />
            </tr>

            <!-- Regular data row -->
            <tr
              v-else
              class="transition-colors hover:bg-muted/30"
              :class="{ 'pl-4': row.level > 0 }"
            >
              <!-- Index cells (row labels) -->
              <template v-if="hasGrouping">
                <!-- Indent for grouped rows -->
                <td
                  v-for="(value, idx) in row.displayIndexValues || row.indexValues"
                  :key="'idx-' + idx"
                  class="border-b border-default/50 px-3 py-2"
                  :class="{ 'pl-8': idx === 0 && row.level > 0 }"
                >
                  {{ formatValue(value, pivotData.indexColumns[idx + row.level]?.dtype) }}
                </td>
              </template>
              <template v-else>
                <td
                  v-for="(value, idx) in row.indexValues"
                  :key="'idx-' + idx"
                  class="border-b border-default/50 px-3 py-2 font-medium"
                >
                  {{ formatValue(value, pivotData.indexColumns[idx]?.dtype) }}
                </td>
              </template>

              <!-- Data value cells -->
              <td
                v-for="(value, idx) in row.dataValues"
                :key="'val-' + idx"
                class="border-b border-default/50 px-3 py-2 text-right tabular-nums"
                :class="{
                  'font-mono': isNumeric(pivotData.valueColumns[idx]?.dtype),
                }"
                :style="getCellStyle(value, idx)"
              >
                {{ formatValue(value, pivotData.valueColumns[idx]?.dtype) }}
              </td>
            </tr>
          </template>

          <!-- Totals Row -->
          <tr v-if="columnTotals" class="bg-primary/10 font-bold">
            <td
              :colspan="pivotData.indexColumns.length"
              class="border-t-2 border-primary/30 px-3 py-2"
            >
              Total
            </td>
            <td
              v-for="(value, idx) in columnTotals"
              :key="'total-' + idx"
              class="border-t-2 border-primary/30 px-3 py-2 text-right font-mono tabular-nums"
            >
              {{ formatValue(value, pivotData.valueColumns[idx]?.dtype) }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.tabular-nums {
  font-variant-numeric: tabular-nums;
}
</style>

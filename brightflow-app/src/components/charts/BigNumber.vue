<script setup lang="ts">
/**
 * Headline-number view of the current results (pivot results preferred over
 * table). The display shape is inferred from the data itself: a single
 * numeric cell renders as one big value, a one-row pivot fans out per column,
 * and a multi-row table falls back to computed sum/avg/min/max/count over the
 * first numeric column.
 */

import { computed } from 'vue';

import EmptyState from '@/components/common/EmptyState.vue';
import { usePivotStore } from '@/stores/pivot';
import { useResultsStore } from '@/stores/results';
import type { LogicalType } from '@/types/generated';
import { isFloatType, isNumericType } from '@/utils/dtype';
import { formatCompact, formatDecimal } from '@/utils/format';

const resultsStore = useResultsStore();
const pivotStore = usePivotStore();

interface DisplayValue {
  label: string;
  value: unknown;
  datatype: LogicalType;
}

interface SingleDisplay {
  type: 'single';
  label: string;
  value: unknown;
  datatype: LogicalType;
}

interface MultiDisplay {
  type: 'multi';
  values: DisplayValue[];
}

interface AggregateDisplay {
  type: 'aggregate';
  label: string;
  primary: { label: string; value: number };
  secondary: { label: string; value: number }[];
  datatype: LogicalType;
}

type DisplayData = SingleDisplay | MultiDisplay | AggregateDisplay | null;

// Get the primary value to display
const displayData = computed((): DisplayData => {
  // Use pivot results if available, otherwise table results
  const hasPivot = resultsStore.hasPivotResults;
  const cols = hasPivot ? resultsStore.pivot.columns : resultsStore.table.columns;
  const rows = hasPivot ? resultsStore.pivot.rows : resultsStore.table.rows;

  if (cols.length === 0 || rows.length === 0) {
    return null;
  }

  // Find numeric columns
  const numericIndices = cols
    .map((col, idx) => ({ col, idx }))
    .filter(({ col }) => isNumericType(col.datatype));

  if (numericIndices.length === 0) {
    return null;
  }

  // For pivot data with one row, show all numeric values
  const firstRow = rows[0];
  if (hasPivot && rows.length === 1 && firstRow) {
    const values = numericIndices.map(({ col, idx }) => ({
      datatype: col.datatype,
      label: col.name,
      value: firstRow[idx],
    }));
    return { type: 'multi', values };
  }

  // For single value pivot/aggregate
  const firstNumeric = numericIndices[0];
  if (rows.length === 1 && numericIndices.length === 1 && firstNumeric && firstRow) {
    return {
      datatype: firstNumeric.col.datatype,
      label: firstNumeric.col.name,
      type: 'single',
      value: firstRow[firstNumeric.idx],
    };
  }

  // For table data, calculate aggregates
  const primaryNumeric = numericIndices[0];
  if (!primaryNumeric) {
    return null;
  }

  const values = rows
    .map((row) => row[primaryNumeric.idx])
    .filter((v): v is number => typeof v === 'number');

  if (values.length === 0) {
    return null;
  }

  const sum = values.reduce((a, b) => a + b, 0);
  const avg = sum / values.length;
  const min = Math.min(...values);
  const max = Math.max(...values);

  return {
    datatype: primaryNumeric.col.datatype,
    label: primaryNumeric.col.name,
    primary: { label: 'Sum', value: sum },
    secondary: [
      { label: 'Average', value: avg },
      { label: 'Min', value: min },
      { label: 'Max', value: max },
      { label: 'Count', value: values.length },
    ],
    type: 'aggregate',
  };
});

// Format number for display (en-US pinned via the shared helpers)
function formatNumber(value: unknown, datatype: LogicalType, compact = false): string {
  if (value === null || value === undefined) {
    return '—';
  }
  if (typeof value !== 'number') {
    return String(value);
  }
  if (compact && Math.abs(value) >= 1000) {
    return formatCompact(value);
  }
  return formatDecimal(value, isFloatType(datatype) ? 2 : 0);
}

// Format large primary number
function formatPrimary(value: unknown, datatype: LogicalType): string {
  return formatNumber(value, datatype, true);
}

// Get aggregation label from pivot config
const aggregationLabel = computed((): string | null => {
  const firstValueField = pivotStore.valueFields[0];
  if (firstValueField) {
    const agg = firstValueField.aggregation;
    const labels: Record<string, string> = {
      avg: 'Average',
      count: 'Count',
      max: 'Maximum',
      mean: 'Mean',
      min: 'Minimum',
      sum: 'Sum',
    };
    return (agg && labels[agg]) ?? agg ?? null;
  }
  return null;
});
</script>

<template>
  <div class="flex h-full items-center justify-center p-8">
    <!-- No data state -->
    <EmptyState
      v-if="!displayData"
      detail="Add a value field to see metrics"
      message="No numeric data"
    />

    <!-- Single value display -->
    <div v-else-if="displayData.type === 'single'" class="text-center">
      <div class="mb-2 text-6xl font-bold text-default tabular-nums">
        {{ formatPrimary(displayData.value, displayData.datatype) }}
      </div>
      <div class="text-lg text-muted">
        {{ aggregationLabel || displayData.label }}
      </div>
    </div>

    <!-- Multi-value display (pivot with one row, multiple values) -->
    <div v-else-if="displayData.type === 'multi'" class="flex flex-wrap justify-center gap-8">
      <div v-for="(item, idx) in displayData.values" :key="idx" class="px-6 text-center">
        <div class="mb-2 text-5xl font-bold text-default tabular-nums">
          {{ formatPrimary(item.value, item.datatype) }}
        </div>
        <div class="text-sm text-muted">{{ item.label }}</div>
      </div>
    </div>

    <!-- Aggregate display (calculated from table data) -->
    <div v-else-if="displayData.type === 'aggregate'" class="text-center">
      <!-- Primary metric -->
      <div class="mb-8">
        <div class="mb-2 text-6xl font-bold text-default tabular-nums">
          {{ formatPrimary(displayData.primary.value, displayData.datatype) }}
        </div>
        <div class="text-lg text-muted">
          {{ displayData.primary.label }} of {{ displayData.label }}
        </div>
      </div>

      <!-- Secondary metrics -->
      <div class="flex justify-center gap-8">
        <div v-for="(item, idx) in displayData.secondary" :key="idx" class="px-4 text-center">
          <div class="text-2xl font-semibold text-default tabular-nums">
            {{ formatNumber(item.value, displayData.datatype) }}
          </div>
          <div class="mt-1 text-sm text-muted">{{ item.label }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tabular-nums {
  font-variant-numeric: tabular-nums;
}
</style>

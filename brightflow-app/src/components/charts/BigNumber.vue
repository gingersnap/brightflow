<script setup lang="ts">
import { computed } from 'vue';
import { useResultsStore } from '@/stores/results';
import { usePivotStore } from '@/stores/pivot';

const resultsStore = useResultsStore();
const pivotStore = usePivotStore();

interface DisplayValue {
  label: string;
  value: unknown;
  dtype: string;
}

interface SingleDisplay {
  type: 'single';
  label: string;
  value: unknown;
  dtype: string;
}

interface MultiDisplay {
  type: 'multi';
  values: DisplayValue[];
}

interface AggregateDisplay {
  type: 'aggregate';
  label: string;
  primary: { label: string; value: number };
  secondary: Array<{ label: string; value: number }>;
  dtype: string;
}

type DisplayData = SingleDisplay | MultiDisplay | AggregateDisplay | null;

// Get the primary value to display
const displayData = computed((): DisplayData => {
  // Use pivot results if available, otherwise table results
  const hasPivot = resultsStore.hasPivotResults;
  const cols = hasPivot ? resultsStore.pivotColumns : resultsStore.tableColumns;
  const rows = hasPivot ? resultsStore.pivotRows : resultsStore.tableRows;

  if (cols.length === 0 || rows.length === 0) return null;

  // Find numeric columns
  const numericIndices = cols
    .map((col, idx) => ({ col, idx }))
    .filter(({ col }) => ['int', 'float', 'decimal', 'number', 'i64', 'f64'].includes(col.dtype));

  if (numericIndices.length === 0) return null;

  // For pivot data with one row, show all numeric values
  const firstRow = rows[0];
  if (hasPivot && rows.length === 1 && firstRow) {
    const values = numericIndices.map(({ col, idx }) => ({
      label: col.name,
      value: firstRow[idx],
      dtype: col.dtype,
    }));
    return { type: 'multi', values };
  }

  // For single value pivot/aggregate
  const firstNumeric = numericIndices[0];
  if (rows.length === 1 && numericIndices.length === 1 && firstNumeric && firstRow) {
    return {
      type: 'single',
      label: firstNumeric.col.name,
      value: firstRow[firstNumeric.idx],
      dtype: firstNumeric.col.dtype,
    };
  }

  // For table data, calculate aggregates
  const primaryNumeric = numericIndices[0];
  if (!primaryNumeric) return null;

  const values = rows
    .map((row) => row[primaryNumeric.idx])
    .filter((v): v is number => typeof v === 'number');

  if (values.length === 0) return null;

  const sum = values.reduce((a, b) => a + b, 0);
  const avg = sum / values.length;
  const min = Math.min(...values);
  const max = Math.max(...values);

  return {
    type: 'aggregate',
    label: primaryNumeric.col.name,
    primary: { label: 'Sum', value: sum },
    secondary: [
      { label: 'Average', value: avg },
      { label: 'Min', value: min },
      { label: 'Max', value: max },
      { label: 'Count', value: values.length },
    ],
    dtype: primaryNumeric.col.dtype,
  };
});

// Format number for display
function formatNumber(value: unknown, dtype: string, compact = false): string {
  if (value === null || value === undefined) return '—';
  if (typeof value !== 'number') return String(value);

  const isFloat = ['float', 'decimal', 'f64'].includes(dtype);

  if (compact && Math.abs(value) >= 1000000) {
    return new Intl.NumberFormat(undefined, {
      notation: 'compact',
      maximumFractionDigits: 1,
    }).format(value);
  }

  if (compact && Math.abs(value) >= 1000) {
    return new Intl.NumberFormat(undefined, {
      notation: 'compact',
      maximumFractionDigits: 1,
    }).format(value);
  }

  return value.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: isFloat ? 2 : 0,
  });
}

// Format large primary number
function formatPrimary(value: unknown, dtype: string): string {
  return formatNumber(value, dtype, true);
}

// Get aggregation label from pivot config
const aggregationLabel = computed((): string | null => {
  const firstValueField = pivotStore.valueFields[0];
  if (firstValueField) {
    const agg = firstValueField.aggregation;
    const labels: Record<string, string> = {
      sum: 'Sum',
      count: 'Count',
      avg: 'Average',
      min: 'Minimum',
      max: 'Maximum',
      mean: 'Mean',
    };
    return (agg && labels[agg]) ?? agg ?? null;
  }
  return null;
});
</script>

<template>
  <div class="flex items-center justify-center h-full p-8">
    <!-- No data state -->
    <div v-if="!displayData" class="text-center text-muted">
      <div class="text-lg mb-2">No numeric data</div>
      <div class="text-sm text-muted/70">Add a value field to see metrics</div>
    </div>

    <!-- Single value display -->
    <div v-else-if="displayData.type === 'single'" class="text-center">
      <div class="text-6xl font-bold text-default tabular-nums mb-2">
        {{ formatPrimary(displayData.value, displayData.dtype) }}
      </div>
      <div class="text-lg text-muted">
        {{ aggregationLabel || displayData.label }}
      </div>
    </div>

    <!-- Multi-value display (pivot with one row, multiple values) -->
    <div v-else-if="displayData.type === 'multi'" class="flex flex-wrap justify-center gap-8">
      <div
        v-for="(item, idx) in displayData.values"
        :key="idx"
        class="text-center px-6"
      >
        <div class="text-5xl font-bold text-default tabular-nums mb-2">
          {{ formatPrimary(item.value, item.dtype) }}
        </div>
        <div class="text-sm text-muted">{{ item.label }}</div>
      </div>
    </div>

    <!-- Aggregate display (calculated from table data) -->
    <div v-else-if="displayData.type === 'aggregate'" class="text-center">
      <!-- Primary metric -->
      <div class="mb-8">
        <div class="text-6xl font-bold text-default tabular-nums mb-2">
          {{ formatPrimary(displayData.primary.value, displayData.dtype) }}
        </div>
        <div class="text-lg text-muted">
          {{ displayData.primary.label }} of {{ displayData.label }}
        </div>
      </div>

      <!-- Secondary metrics -->
      <div class="flex justify-center gap-8">
        <div
          v-for="(item, idx) in displayData.secondary"
          :key="idx"
          class="text-center px-4"
        >
          <div class="text-2xl font-semibold text-default tabular-nums">
            {{ formatNumber(item.value, displayData.dtype) }}
          </div>
          <div class="text-xs text-muted mt-1">{{ item.label }}</div>
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

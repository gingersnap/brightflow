<script setup lang="ts">
import { BarChart, LineChart, PieChart, ScatterChart } from 'echarts/charts';
import {
  GridComponent,
  LegendComponent,
  TitleComponent,
  TooltipComponent,
} from 'echarts/components';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { computed, ref, watch } from 'vue';
import VChart from 'vue-echarts';

import { usePivotStore } from '@/stores/pivot';
import { useResultsStore } from '@/stores/results';
import { useUiStore } from '@/stores/ui';
import type { ChartType } from '@/types';

// Register ECharts components
use([
  CanvasRenderer,
  BarChart,
  LineChart,
  PieChart,
  ScatterChart,
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent,
]);

const resultsStore = useResultsStore();
const uiStore = useUiStore();
const pivotStore = usePivotStore();

// Use pivot data when available, otherwise table data
const chartColumns = computed(() => {
  if (pivotStore.isConfigured && resultsStore.hasPivotResults) {
    return resultsStore.pivotColumns;
  }
  return resultsStore.tableColumns;
});

const chartRows = computed(() => {
  if (pivotStore.isConfigured && resultsStore.hasPivotResults) {
    return resultsStore.pivotRows;
  }
  return resultsStore.tableRows;
});

const hasChartData = computed(() => {
  if (pivotStore.isConfigured && resultsStore.hasPivotResults) {
    return true;
  }
  return resultsStore.hasTableResults;
});

const chartTypes = [
  { label: 'Bar', value: 'bar' },
  { label: 'Line', value: 'line' },
  { label: 'Pie', value: 'pie' },
  { label: 'Scatter', value: 'scatter' },
];

const stackOptions = [
  { label: 'None', value: 'none' },
  { label: 'Stacked', value: 'stacked' },
  { label: '100%', value: 'percent' },
];

// Chart settings
const stacking = ref('none');
const showValues = ref(false);
const horizontal = ref(false);

// Find suitable columns for chart (from pivot or table data)
const stringColumns = computed(() =>
  chartColumns.value
    .filter((c) => ['string', 'text', 'varchar'].includes(c.dtype))
    .map((c) => c.name),
);

const numericColumns = computed(() =>
  chartColumns.value
    .filter((c) => ['int', 'float', 'decimal', 'number', 'i64', 'f64'].includes(c.dtype))
    .map((c) => c.name),
);

const allColumnNames = computed(() => chartColumns.value.map((c) => c.name));

// Chart axis selections
const xAxis = ref<string | null>(null);
const yAxes = ref<string[]>([]);

// Detect if this is pivot data with column breakdown (multiple value columns)
const isPivotWithColumns = computed(
  () =>
    pivotStore.isConfigured && pivotStore.columnFields.length > 0 && resultsStore.hasPivotResults,
);

// Get the index/row columns from pivot (these should be X axis candidates)
const pivotIndexColumns = computed(() => {
  if (!isPivotWithColumns.value) {
    return [];
  }
  return pivotStore.rowFields.map((f) => f.column);
});

// Auto-select axes when data changes
watch(
  () => [stringColumns.value, numericColumns.value, isPivotWithColumns.value, chartColumns.value],
  () => {
    // For pivot with columns, auto-select index column as X and all numeric as Y
    if (isPivotWithColumns.value && chartColumns.value.length > 0) {
      // X axis: first index column (row field)
      const firstPivotIndex = pivotIndexColumns.value[0];
      const firstString = stringColumns.value[0];
      if (firstPivotIndex) {
        xAxis.value = firstPivotIndex;
      } else if (firstString) {
        xAxis.value = firstString;
      }
      // Y axes: all numeric columns (the pivoted values)
      if (numericColumns.value.length > 0) {
        yAxes.value = [...numericColumns.value];
        // Auto-enable stacking for pivot data
        if (stacking.value === 'none' && numericColumns.value.length > 1) {
          stacking.value = 'stacked';
        }
      }
    } else {
      // Regular data - select first of each
      const firstString = stringColumns.value[0];
      const firstNumeric = numericColumns.value[0];
      if (firstString && !xAxis.value) {
        xAxis.value = firstString;
      }
      if (firstNumeric && yAxes.value.length === 0) {
        yAxes.value = [firstNumeric];
      }
    }
  },
  { immediate: true },
);

// Select all numeric columns as Y axes
function selectAllYAxes(): void {
  yAxes.value = [...numericColumns.value];
}

// Check if all numeric columns are selected
const allYAxesSelected = computed(
  () =>
    numericColumns.value.length > 0 && numericColumns.value.every((c) => yAxes.value.includes(c)),
);

// Color palette
const colors = [
  '#5470c6',
  '#91cc75',
  '#fac858',
  '#ee6666',
  '#73c0de',
  '#3ba272',
  '#fc8452',
  '#9a60b4',
  '#ea7ccc',
];

interface SeriesItem {
  name: string | undefined;
  type: string;
  data: unknown[];
  itemStyle: { color: string | undefined };
  stack?: string;
  stackStrategy?: string;
  label?: { show: boolean; position: string; fontSize: number };
  smooth?: boolean;
  areaStyle?: Record<string, unknown>;
}

// Build ECharts options
const chartOption = computed(() => {
  if (!xAxis.value || yAxes.value.length === 0 || !hasChartData.value) {
    return null;
  }

  const xIndex = chartColumns.value.findIndex((c) => c.name === xAxis.value);
  if (xIndex === -1) {
    return null;
  }

  const yIndices = yAxes.value.map((y) => chartColumns.value.findIndex((c) => c.name === y));
  if (yIndices.some((i) => i === -1)) {
    return null;
  }

  const xData = chartRows.value.map((row) => row[xIndex]);

  const baseOption = {
    color: colors,
    grid: {
      left: '3%',
      right: '4%',
      bottom: yAxes.value.length > 1 ? '10%' : '3%',
      containLabel: true,
    },
    legend:
      yAxes.value.length > 1
        ? {
            data: yAxes.value,
            bottom: 0,
          }
        : undefined,
    tooltip: {
      trigger: 'axis' as const,
      axisPointer: { type: 'shadow' as const },
    },
  };

  // Build series for each Y axis
  const buildSeries = (type: string): SeriesItem[] =>
    yIndices.map((yIdx, i) => {
      const yData = chartRows.value.map((row) => row[yIdx]);
      const series: SeriesItem = {
        name: yAxes.value[i],
        type,
        data: yData,
        itemStyle: { color: colors[i % colors.length] },
      };

      // Stacking for bar/line
      if (stacking.value !== 'none' && (type === 'bar' || type === 'line')) {
        series.stack = 'total';
        if (stacking.value === 'percent') {
          series.stackStrategy = 'all';
        }
      }

      // Show values on data points
      if (showValues.value) {
        series.label = {
          show: true,
          position: 'top',
          fontSize: 10,
        };
      }

      // Area style for line charts
      if (type === 'line') {
        series.smooth = true;
        if (stacking.value !== 'none') {
          series.areaStyle = {};
        }
      }

      return series;
    });

  const firstYIdx = yIndices[0];
  const firstYAxis = yAxes.value[0];

  switch (uiStore.chartType) {
    case 'bar': {
      if (horizontal.value) {
        return {
          ...baseOption,
          xAxis: { type: 'value' as const },
          yAxis: {
            type: 'category' as const,
            data: xData,
            axisLabel: { width: 100, overflow: 'truncate' as const },
          },
          series: buildSeries('bar'),
        };
      }
      return {
        ...baseOption,
        xAxis: {
          type: 'category' as const,
          data: xData,
          axisLabel: { rotate: xData.length > 10 ? 45 : 0 },
        },
        yAxis: { type: 'value' as const },
        series: buildSeries('bar'),
      };
    }

    case 'line': {
      return {
        ...baseOption,
        xAxis: {
          type: 'category' as const,
          data: xData,
          boundaryGap: false,
        },
        yAxis: { type: 'value' as const },
        series: buildSeries('line'),
      };
    }

    case 'pie': {
      const yData = firstYIdx !== undefined ? chartRows.value.map((row) => row[firstYIdx]) : [];
      return {
        color: colors,
        legend: { orient: 'vertical' as const, left: 'left' },
        series: [
          {
            type: 'pie' as const,
            radius: ['40%', '70%'],
            data: xData.map((name, i) => ({
              name: String(name),
              value: yData[i],
            })),
            label: showValues.value
              ? {
                  show: true,
                  formatter: '{b}: {d}%',
                }
              : { show: false },
          },
        ],
        tooltip: { trigger: 'item' as const, formatter: '{b}: {c} ({d}%)' },
      };
    }

    case 'scatter': {
      return {
        ...baseOption,
        xAxis: { type: 'value' as const, name: xAxis.value },
        yAxis: { type: 'value' as const, name: firstYAxis },
        series: [
          {
            type: 'scatter' as const,
            data:
              firstYIdx !== undefined
                ? chartRows.value.map((row) => [row[xIndex], row[firstYIdx]])
                : [],
            itemStyle: { color: colors[0] },
          },
        ],
      };
    }

    default: {
      return null;
    }
  }
});

const canShowChart = computed(() => hasChartData.value && numericColumns.value.length > 0);

// Show stacking options only for bar/line
const showStackingOptions = computed(() => ['bar', 'line'].includes(uiStore.chartType));

// Show horizontal option only for bar
const showHorizontalOption = computed(() => uiStore.chartType === 'bar');
</script>

<template>
  <div class="flex h-full flex-col p-4">
    <!-- Chart config -->
    <div class="mb-4 flex flex-wrap items-center gap-4">
      <!-- Chart type -->
      <div class="flex items-center gap-2">
        <label class="text-xs text-muted">Type:</label>
        <USelectMenu
          :model-value="uiStore.chartType"
          :items="chartTypes"
          value-key="value"
          class="w-24"
          size="xs"
          @update:model-value="(val: ChartType) => uiStore.setChartType(val)"
        />
      </div>

      <!-- X Axis -->
      <div class="flex items-center gap-2">
        <label class="text-xs text-muted">X:</label>
        <USelectMenu
          :model-value="xAxis ?? ''"
          :items="stringColumns.length ? stringColumns : allColumnNames"
          placeholder="X axis"
          class="w-32"
          size="xs"
          @update:model-value="(val: string) => (xAxis = val)"
        />
      </div>

      <!-- Y Axes (multi-select) -->
      <div class="flex items-center gap-2">
        <label class="text-xs text-muted">Y:</label>
        <USelectMenu
          v-model="yAxes"
          :items="numericColumns"
          placeholder="Y axis"
          multiple
          class="w-40"
          size="xs"
        />
        <button
          v-if="numericColumns.length > 1 && !allYAxesSelected"
          class="text-xs text-primary transition-colors hover:text-primary/80"
          @click="selectAllYAxes"
        >
          All
        </button>
      </div>

      <!-- Stacking (for bar/line) -->
      <div v-if="showStackingOptions" class="flex items-center gap-2">
        <label class="text-xs text-muted">Stack:</label>
        <USelectMenu
          v-model="stacking"
          :items="stackOptions"
          value-key="value"
          class="w-24"
          size="xs"
        />
      </div>

      <!-- Horizontal toggle (for bar) -->
      <label
        v-if="showHorizontalOption"
        class="flex cursor-pointer items-center gap-1.5 text-xs text-muted"
      >
        <USwitch v-model="horizontal" size="xs" />
        Horizontal
      </label>

      <!-- Show values toggle -->
      <label class="flex cursor-pointer items-center gap-1.5 text-xs text-muted">
        <USwitch v-model="showValues" size="xs" />
        Values
      </label>
    </div>

    <!-- Chart -->
    <div class="min-h-0 flex-1">
      <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />

      <div v-else-if="!canShowChart" class="flex h-full items-center justify-center text-muted">
        <div class="text-center">
          <div class="mb-2">Cannot display chart</div>
          <div class="text-sm text-muted/70">Requires at least one numeric column for Y axis</div>
        </div>
      </div>
    </div>
  </div>
</template>

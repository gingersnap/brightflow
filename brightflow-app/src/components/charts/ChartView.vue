<script setup lang="ts">
/**
 * ECharts view over the current results, using pivot output when a pivot is
 * configured. For a pivot the buckets map the way Excel's do — the first
 * row field is the axis, the column field is the legend, the value is the
 * series — and rows and series follow the field sorts, the same order the
 * pivot table shows. Stacking has three states, decided by bucket shape:
 *
 * - none: one bar per axis item (Excel);
 * - stacked with a column field: one series per column value, legend shown
 *   (Excel); colour follows the bar when every series lives in one bar
 *   (a parent/child pair built wide), otherwise the palette per series with
 *   the smallest series past the palette folded into one "Other";
 * - stacked with no column field and two row fields: the inner row field
 *   becomes the segments of the outer field's bar, series by position, no
 *   legend, hover and labels name the segment. Excel has no chart for this
 *   shape; it is the chart of the nested pivot table.
 *
 * Hand-built tables (no pivot) keep the old behaviour: first string column
 * as X, first numeric as Y, both overridable from the toolbar. The rules
 * live in `chartColor.ts` and `stackedRows.ts`; this file only wires them.
 */

import { useCssVar } from '@vueuse/core';
import { computed, ref, watch } from 'vue';

import '@/services/echarts';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import { useDatasetStore } from '@/stores/dataset';
import { usePivotStore } from '@/stores/pivot';
import { useResultsStore } from '@/stores/results';
import { useUiStore } from '@/stores/ui';
import type { ChartType } from '@/types';
import { fieldKey } from '@/utils/buildOperations';
import { isNumericDtype, isStringDtype } from '@/utils/dtype';
import { humanizePeriod, humanizePeriodShort } from '@/utils/format';
import { DEFAULT_FIELD_SORT, orderRows, orderSeries } from '@/utils/pivotOrder';

import {
  foldSeries,
  foldedColumn,
  isOtherName,
  opacityForRank,
  rankWithinBars,
  seriesConfinedToOneBar,
} from './chartColor';
import { stackedRowSeries } from './stackedRows';

const resultsStore = useResultsStore();
const uiStore = useUiStore();
const pivotStore = usePivotStore();
const datasetStore = useDatasetStore();
const colors = useChartColors();
// `other`, null and folded series wear the muted text colour at every level.
const mutedVar = useCssVar('--ui-text-muted', document.documentElement, { observe: true });
const muted = computed(() => (mutedVar.value ?? '').trim() || '#9ca3af');

/** Share of its bar a segment needs before its label is drawn. */
const LABEL_MIN_SHARE = 0.08;

const usingPivot = computed(() => pivotStore.isConfigured && resultsStore.hasPivotResults);

// Use pivot data when available, otherwise table data
const chartColumns = computed(() =>
  usingPivot.value ? resultsStore.pivot.columns : resultsStore.table.columns,
);
const chartRows = computed(() =>
  usingPivot.value ? resultsStore.pivot.rows : resultsStore.table.rows,
);
const hasChartData = computed(() => usingPivot.value || resultsStore.hasTableResults);

// Typed so USelectMenu's update:model-value emits ChartType, not string.
const chartTypes: { label: string; value: ChartType }[] = [
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
  chartColumns.value.filter((c) => isStringDtype(c.dtype)).map((c) => c.name),
);

const numericColumns = computed(() =>
  chartColumns.value.filter((c) => isNumericDtype(c.dtype)).map((c) => c.name),
);

const allColumnNames = computed(() => chartColumns.value.map((c) => c.name));

// Chart axis selections
const xAxis = ref<string | null>(null);
const yAxes = ref<string[]>([]);

/** Result-set indices of the row fields, in nesting order. */
const rowFieldIdx = computed(() =>
  usingPivot.value
    ? pivotStore.rowFields
        .map((f) => allColumnNames.value.indexOf(fieldKey(f)))
        .filter((i) => i !== -1)
    : [],
);

/** The X axis is a bucketed time field: its labels are period strings. */
const xIsPeriod = computed(() => usingPivot.value && pivotStore.rowFields[0]?.granularity != null);

/** Axis tick: compact period when bucketed, the raw value otherwise. */
function xTick(value: string): string {
  return xIsPeriod.value ? humanizePeriodShort(value) : value;
}
const hasColumnField = computed(() => usingPivot.value && pivotStore.columnFields.length > 0);

/** The three stacking states of the header comment (`flat` = no stacking). */
type StackState = 'flat' | 'legend' | 'innerRow';
const stackState = computed<StackState>(() => {
  if (!usingPivot.value || stacking.value === 'none') {
    return 'flat';
  }
  if (hasColumnField.value) {
    return 'legend';
  }
  if (uiStore.chartType === 'bar' && rowFieldIdx.value.length >= 2) {
    return 'innerRow';
  }
  return 'flat';
});

/* Auto-select axes when data changes. For a pivot the buckets decide: X is
   the first row field and every value column is a Y; stacking turns on
   wherever the shape has something to stack. */
watch(
  () => [chartColumns.value, usingPivot.value, rowFieldIdx.value],
  () => {
    if (usingPivot.value && chartColumns.value.length > 0) {
      const firstRow = rowFieldIdx.value[0];
      xAxis.value =
        firstRow == null
          ? (stringColumns.value[0] ?? null)
          : (allColumnNames.value[firstRow] ?? null);
      yAxes.value = [...numericColumns.value];
      const stackable = numericColumns.value.length > 1 || rowFieldIdx.value.length >= 2;
      if (stacking.value === 'none' && stackable) {
        stacking.value = 'stacked';
      }
      return;
    }
    // Regular data - select first of each
    const firstString = stringColumns.value[0];
    const firstNumeric = numericColumns.value[0];
    if (firstString && !xAxis.value) {
      xAxis.value = firstString;
    }
    if (firstNumeric && yAxes.value.length === 0) {
      yAxes.value = [firstNumeric];
    }
  },
  { immediate: true },
);

/* Outer labels run long; the first time a nested pivot stacks, lay it on
   its side. The switch still overrides afterwards. */
watch(stackState, (state) => {
  if (state === 'innerRow') {
    horizontal.value = true;
  }
});

// Select all numeric columns as Y axes
function selectAllYAxes(): void {
  yAxes.value = [...numericColumns.value];
}

// Check if all numeric columns are selected
const allYAxesSelected = computed(
  () =>
    numericColumns.value.length > 0 && numericColumns.value.every((c) => yAxes.value.includes(c)),
);

interface SeriesItem {
  name: string | undefined;
  type: string;
  data: unknown[];
  itemStyle: { color: string | undefined; opacity?: number };
  stack?: string;
  stackStrategy?: string;
  label?: Record<string, unknown>;
  smooth?: boolean;
  areaStyle?: Record<string, unknown>;
}

/** One stacked segment as ECharts sees it in a by-position series. */
interface SegmentDatum {
  value: number;
  name: string;
  share: number;
  itemStyle: { color: string; opacity: number };
}

/** The slice of an ECharts tooltip/label callback parameter this file reads. */
interface Param {
  axisValueLabel?: string;
  seriesName?: string;
  value?: unknown;
  data?: unknown;
}

function isSegment(d: unknown): d is SegmentDatum {
  return typeof d === 'object' && d != null && 'share' in d && 'name' in d;
}

function pct(share: number): string {
  return `${Math.round(share * 100)}%`;
}

/** Tooltip over a stack: every non-zero segment with its share of the bar. */
function stackTooltip(params: unknown): string {
  const list = (Array.isArray(params) ? params : [params]) as Param[];
  const rawHead = list[0]?.axisValueLabel ?? '';
  const head = xIsPeriod.value ? humanizePeriod(rawHead) : rawHead;
  const rows: { name: string; value: number }[] = [];
  for (const p of list) {
    if (isSegment(p.data)) {
      if (p.data.value > 0) {
        rows.push({ name: p.data.name, value: p.data.value });
      }
    } else if (typeof p.value === 'number' && p.value > 0) {
      rows.push({ name: p.seriesName ?? '', value: p.value });
    }
  }
  const total = rows.reduce((sum, r) => sum + r.value, 0);
  const lines = rows.map(
    (r) => `${r.name}: ${r.value.toLocaleString()} (${pct(total === 0 ? 0 : r.value / total)})`,
  );
  return [`<b>${head}</b> · ${total.toLocaleString()}`, ...lines].join('<br/>');
}

/** Segment label: its name (and value when asked) once it is wide enough. */
function segmentLabel(p: Param): string {
  if (!isSegment(p.data) || p.data.share < LABEL_MIN_SHARE) {
    return '';
  }
  return showValues.value ? `${p.data.name} ${p.data.value.toLocaleString()}` : p.data.name;
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

  const unorderedY = yAxes.value.map((y) => chartColumns.value.findIndex((c) => c.name === y));
  if (unorderedY.some((i) => i === -1)) {
    return null;
  }

  // Pivot results follow the field sorts; hand-built tables keep query order.
  const names = allColumnNames.value;
  const yIndices = usingPivot.value
    ? orderSeries({
        rows: chartRows.value,
        seriesIdx: unorderedY,
        names,
        sort: pivotStore.columnFields[0]?.sort ?? DEFAULT_FIELD_SORT,
      })
    : unorderedY;
  const rowOrder = usingPivot.value
    ? orderRows({
        rows: chartRows.value,
        indexIdx: rowFieldIdx.value,
        valueIdx: yIndices,
        sorts: pivotStore.rowFields.map((f) => f.sort ?? DEFAULT_FIELD_SORT),
      })
    : chartRows.value.map((_, i) => i);
  const orderedRows = rowOrder.map((i) => chartRows.value[i] ?? []);
  const state = stackState.value;
  const stacked = state !== 'flat';
  const stackStrategy = stacking.value === 'percent' ? 'all' : undefined;

  const categoryAxis = (data: unknown[]) =>
    horizontal.value
      ? {
          xAxis: { type: 'value' as const },
          yAxis: {
            type: 'category' as const,
            data,
            // Largest first reads top-down; ECharts draws category 0 at the bottom.
            inverse: true,
            axisLabel: { width: 140, overflow: 'truncate' as const, formatter: xTick },
          },
        }
      : {
          xAxis: {
            type: 'category' as const,
            data,
            axisLabel: { rotate: data.length > 10 ? 45 : 0, formatter: xTick },
          },
          yAxis: { type: 'value' as const },
        };

  // --- State: inner row field stacked inside the outer bar -----------------
  if (state === 'innerRow') {
    const [outerIdx, innerIdx] = rowFieldIdx.value;
    const valueIdx = yIndices[0];
    if (outerIdx == null || innerIdx == null || valueIdx == null) {
      return null;
    }
    const rows = stackedRowSeries({
      rows: chartRows.value,
      outerIdx,
      innerIdx,
      valueIdx,
      order: rowOrder,
    });
    const series = rows.series.map((s) => ({
      name: `#${s.rank + 1}`,
      type: 'bar' as const,
      stack: 'total',
      stackStrategy,
      data: s.data.map((seg, bar): SegmentDatum | null => {
        if (seg == null) {
          return null;
        }
        const count = rows.segments[bar]?.length ?? 1;
        const other = isOtherName(seg.name);
        return {
          value: seg.value,
          name: seg.name,
          share: seg.share,
          itemStyle: {
            color: other ? muted.value : (colors[bar % colors.length] ?? muted.value),
            opacity: other ? 1 : opacityForRank(s.rank, count),
          },
        };
      }),
      label: { show: true, position: 'inside', fontSize: 10, formatter: segmentLabel },
    }));
    return {
      grid: { left: '3%', right: '4%', bottom: '3%', containLabel: true },
      tooltip: {
        trigger: 'axis' as const,
        axisPointer: { type: 'shadow' as const },
        formatter: stackTooltip,
      },
      ...categoryAxis(rows.bars),
      series,
    };
  }

  // --- States: flat and legend ----------------------------------------------
  // Colour per series: bar hue with shades when every series lives in one
  // Bar; otherwise the palette in order, never cycled — surplus series fold.
  let seriesIdx = yIndices;
  let extra: { name: string; data: number[] } | null = null;
  const styleFor = new Map<number, { color: string; opacity?: number }>();
  const home = state === 'legend' ? seriesConfinedToOneBar(orderedRows, yIndices, names) : null;
  if (home == null) {
    const fold = foldSeries(yIndices, names, colors.length - 1);
    seriesIdx = fold.kept;
    for (const [i, s] of fold.kept.entries()) {
      styleFor.set(s, {
        color: isOtherName(names[s]) ? muted.value : (colors[i] ?? muted.value),
      });
    }
    if (fold.folded.length > 0) {
      extra = {
        name: `Other (${fold.folded.length} more)`,
        data: foldedColumn(orderedRows, fold.folded),
      };
    }
  } else {
    const ranks = rankWithinBars(orderedRows, yIndices, home);
    for (const s of yIndices) {
      const bar = home.get(s) ?? -1;
      const rank = ranks.get(s);
      styleFor.set(s, {
        color: bar === -1 ? muted.value : (colors[bar % colors.length] ?? muted.value),
        ...(rank == null ? {} : { opacity: opacityForRank(rank.rank, rank.count) }),
      });
    }
  }
  const yNames = seriesIdx.map((i) => names[i] ?? '');
  const legendNames = extra == null ? yNames : [...yNames, extra.name];

  const xData = orderedRows.map((row) => row[xIndex]);

  const baseOption = {
    color: colors,
    grid: {
      left: '3%',
      right: '4%',
      bottom: legendNames.length > 1 ? '10%' : '3%',
      containLabel: true,
    },
    legend:
      legendNames.length > 1
        ? {
            type: 'scroll' as const,
            data: legendNames,
            bottom: 0,
          }
        : undefined,
    tooltip: {
      trigger: 'axis' as const,
      axisPointer: { type: 'shadow' as const },
      ...(stacked ? { formatter: stackTooltip } : {}),
    },
  };

  // Build series for each Y axis
  const buildSeries = (type: string): SeriesItem[] => {
    const decorate = (series: SeriesItem): SeriesItem => {
      // Stacking for bar/line
      if (stacked && (type === 'bar' || type === 'line')) {
        series.stack = 'total';
        if (stackStrategy != null) {
          series.stackStrategy = stackStrategy;
        }
      }
      // Show values on data points
      if (showValues.value) {
        series.label = {
          show: true,
          position: stacked ? 'inside' : 'top',
          fontSize: 10,
        };
      }
      // Area style for line charts
      if (type === 'line') {
        series.smooth = true;
        if (stacked) {
          series.areaStyle = {};
        }
      }
      return series;
    };
    const list = seriesIdx.map((yIdx, i) =>
      decorate({
        name: yNames[i],
        type,
        data: orderedRows.map((row) => row[yIdx]),
        itemStyle: styleFor.get(yIdx) ?? { color: colors[i % colors.length] },
      }),
    );
    if (extra != null) {
      list.push(
        decorate({
          name: extra.name,
          type,
          data: extra.data,
          itemStyle: { color: muted.value },
        }),
      );
    }
    return list;
  };

  const firstYIdx = seriesIdx[0];
  const firstYAxis = yNames[0];

  switch (uiStore.chartType) {
    case 'bar': {
      return { ...baseOption, ...categoryAxis(xData), series: buildSeries('bar') };
    }

    case 'line': {
      return {
        ...baseOption,
        xAxis: {
          type: 'category' as const,
          data: xData,
          boundaryGap: false,
          axisLabel: { formatter: xTick },
        },
        yAxis: { type: 'value' as const },
        series: buildSeries('line'),
      };
    }

    case 'pie': {
      const yData = firstYIdx === undefined ? [] : orderedRows.map((row) => row[firstYIdx]);
      return {
        color: colors,
        legend: { type: 'scroll' as const, orient: 'vertical' as const, left: 'left' },
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
        xAxis: {
          type: 'value' as const,
          name: xAxis.value == null ? undefined : datasetStore.labelFor(xAxis.value),
        },
        yAxis: {
          type: 'value' as const,
          name: firstYAxis == null ? undefined : datasetStore.labelFor(firstYAxis),
        },
        series: [
          {
            type: 'scatter' as const,
            data:
              firstYIdx === undefined
                ? []
                : orderedRows.map((row) => [row[xIndex], row[firstYIdx]]),
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
        <label class="text-sm text-muted">Type:</label>
        <USelectMenu
          :model-value="uiStore.chartType"
          :items="chartTypes"
          value-key="value"
          class="w-24"
          size="xs"
          @update:model-value="(val: ChartType) => (uiStore.chartType = val)"
        />
      </div>

      <!-- X Axis -->
      <div class="flex items-center gap-2">
        <label class="text-sm text-muted">X:</label>
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
        <label class="text-sm text-muted">Y:</label>
        <USelectMenu
          v-model="yAxes"
          :items="numericColumns"
          placeholder="Y axis"
          multiple
          class="w-40"
          size="xs"
        />
        <!-- Raw text link: a UButton here would out-weigh the xs USelect it
             annotates; this is a chrome-adjacent micro affordance -->
        <button
          v-if="numericColumns.length > 1 && !allYAxesSelected"
          class="text-sm text-primary transition-colors hover:text-primary/80"
          @click="selectAllYAxes"
        >
          All
        </button>
      </div>

      <!-- Stacking (for bar/line) -->
      <div v-if="showStackingOptions" class="flex items-center gap-2">
        <label class="text-sm text-muted">Stack:</label>
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
        class="flex cursor-pointer items-center gap-1.5 text-sm text-muted"
      >
        <USwitch v-model="horizontal" size="xs" />
        Horizontal
      </label>

      <!-- Show values toggle -->
      <label class="flex cursor-pointer items-center gap-1.5 text-sm text-muted">
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

<script setup lang="ts">
/**
 * History line with one extra category tick appended for the forecast
 * period: the expected and actual values as marked dots (actual in fixed
 * red), and the prediction interval shaded only at that final tick. All the
 * marks land on the appended slot by null-padding each series to the
 * history's length, so the miss is read as actual-vs-expected inside (or
 * outside) the band.
 */

import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/types/generated';
import { formatCompact, formatNumber, humanizeColumn, humanizePeriodShort } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import '@/services/echarts';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'Forecast') {
    return null;
  }
  const a = props.node.analysis;
  const forecastTick = a.type === 'ForecastDeviation' ? humanizePeriodShort(a.period) : 'forecast';
  // Append predicted point
  const labels = [...data.labels.map((l) => humanizePeriodShort(l)), forecastTick];
  const histPadded: (number | null)[] = [...data.history, null];
  const expectedSeries: (number | null)[] = data.history.map(() => null);
  expectedSeries.push(data.expected);
  const actualSeries: (number | null)[] = data.history.map(() => null);
  actualSeries.push(data.actual);

  // PI band: show as range only at the last point
  const piLow: (number | null)[] = data.history.map(() => null);
  piLow.push(data.pi_low);
  const piHigh: (number | null)[] = data.history.map(() => null);
  piHigh.push(data.pi_high);
  return {
    grid: { bottom: 24, containLabel: true, left: 8, right: 8, top: 24 },
    series: [
      {
        data: histPadded,
        lineStyle: { color: colors[0] ?? '#3b82f6', width: 2 },
        showSymbol: false,
        type: 'line',
      },
      {
        data: expectedSeries,
        lineStyle: { color: colors[4] ?? '#f59e0b', type: 'dashed', width: 0 },
        markPoint: {
          data: [
            {
              coord: [labels.length - 1, data.expected],
              itemStyle: { color: colors[4] ?? '#f59e0b' },
              symbol: 'circle',
              symbolSize: 8,
            },
          ],
          label: { show: false },
        },
        showSymbol: false,
        type: 'line',
      },
      {
        data: actualSeries,
        lineStyle: { color: '#ef4444', width: 0 },
        markPoint: {
          data: [
            {
              coord: [labels.length - 1, data.actual],
              itemStyle: { color: '#ef4444' },
              symbol: 'circle',
              symbolSize: 10,
            },
          ],
          label: { show: false },
        },
        showSymbol: false,
        type: 'line',
      },
      {
        data: piLow,
        lineStyle: { opacity: 0 },
        showSymbol: false,
        silent: true,
        stack: 'pi',
        type: 'line',
      },
      {
        areaStyle: { color: `${colors[4] ?? '#f59e0b'}22` },
        data: piHigh.map((h, i) => (h === null ? null : h - (piLow[i] ?? 0))),
        lineStyle: { opacity: 0 },
        showSymbol: false,
        silent: true,
        stack: 'pi',
        type: 'line',
      },
    ],
    tooltip: {
      trigger: 'axis',
      valueFormatter: (v: number | null) => (v == null ? '' : formatNumber(v)),
    },
    xAxis: {
      axisLabel: { fontSize: 10 },
      boundaryGap: false,
      data: labels,
      type: 'category',
    },
    yAxis: {
      axisLabel: { fontSize: 10, formatter: (v: number) => formatCompact(v) },
      name: humanizeColumn(measureOf(props.node)),
      nameGap: 12,
      nameTextStyle: { fontSize: 10 },
      scale: true,
      type: 'value',
    },
  };
});
</script>

<template>
  <div class="h-40 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No forecast data available
    </div>
  </div>
</template>

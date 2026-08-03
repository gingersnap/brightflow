<script setup lang="ts">
/**
 * Line chart of the full series with the expected band_low/band_high range
 * shaded via two invisible stacked lines, and the anomalous point flagged as
 * a red markPoint — the band gives the reader the "normal" envelope the
 * marked point escaped.
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
  if (!data || data.type !== 'Series') {
    return null;
  }
  const labels = data.labels;
  const values = data.values;
  const bandLow = data.band_low;
  const bandHigh = data.band_high;
  const marker = data.marker_index;

  const series: Record<string, unknown>[] = [];

  if (bandLow && bandHigh) {
    series.push(
      {
        type: 'line',
        data: bandLow,
        stack: 'band',
        lineStyle: { opacity: 0 },
        symbol: 'none',
        silent: true,
      },
      {
        type: 'line',
        data: bandHigh.map((h, i) => h - (bandLow[i] ?? 0)),
        stack: 'band',
        lineStyle: { opacity: 0 },
        areaStyle: { color: `${colors[3] ?? '#888'}22` },
        symbol: 'none',
        silent: true,
      },
    );
  }

  series.push({
    type: 'line',
    data: values,
    smooth: false,
    showSymbol: false,
    lineStyle: { width: 2, color: colors[0] ?? '#3b82f6' },
    markPoint:
      marker !== null && marker !== undefined
        ? {
            symbolSize: 14,
            data: [{ coord: [marker, values[marker] ?? 0], itemStyle: { color: '#ef4444' } }],
            label: { show: false },
          }
        : undefined,
  });

  return {
    grid: { bottom: 24, containLabel: true, left: 8, right: 8, top: 24 },
    series,
    tooltip: {
      axisPointer: { type: 'cross' },
      trigger: 'axis',
      valueFormatter: (v: number) => formatNumber(v),
    },
    xAxis: {
      axisLabel: { fontSize: 10, formatter: (l: string) => humanizePeriodShort(l) },
      boundaryGap: false,
      data: labels,
      type: 'category',
    },
    yAxis: {
      axisLabel: { fontSize: 10, formatter: (v: number) => formatCompact(v) },
      name: data.y_label ?? humanizeColumn(measureOf(props.node)),
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
      No chart data available
    </div>
  </div>
</template>

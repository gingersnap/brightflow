<script setup lang="ts">
/**
 * Line chart of the full series with the expected band_low/band_high range
 * shaded via two invisible stacked lines, and the anomalous point flagged as
 * a red markPoint — the band gives the reader the "normal" envelope the
 * marked point escaped.
 */

import { computed } from 'vue';
import VChart from 'vue-echarts';

import EmptyState from '@/components/common/EmptyState.vue';
import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/types/generated';
import { humanizeColumn, humanizePeriodShort } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import { periodSeriesOption } from './periodSeriesOption';
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

  return periodSeriesOption({
    boundaryGap: false,
    labels,
    series,
    tooltip: { axisPointer: { type: 'cross' } },
    xFormatter: (l: string) => humanizePeriodShort(l),
    yName: data.y_label ?? humanizeColumn(measureOf(props.node)),
    yScale: true,
  });
});
</script>

<template>
  <div class="h-40 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <EmptyState v-else class="h-full" message="No chart data available" />
  </div>
</template>

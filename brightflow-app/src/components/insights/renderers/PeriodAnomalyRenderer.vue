<script setup lang="ts">
/**
 * One bar per period, with the flagged period singled out purely by color —
 * red at the marker index, muted grey everywhere else — so the whole visual
 * is the flagged period's height against its ordinary neighbors.
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
  const marker = data.marker_index;

  const itemColor = (i: number): string => {
    if (i === marker) {
      return '#ef4444';
    }
    return colors[2] ?? '#94a3b8';
  };
  return {
    grid: { bottom: 30, containLabel: true, left: 8, right: 8, top: 24 },
    series: [
      {
        data: values.map((v, i) => ({ itemStyle: { color: itemColor(i) }, value: v })),
        type: 'bar',
      },
    ],
    tooltip: { trigger: 'axis', valueFormatter: (v: number) => formatNumber(v) },
    xAxis: {
      axisLabel: { fontSize: 10, formatter: (l: string) => humanizePeriodShort(l), rotate: 30 },
      data: labels,
      type: 'category',
    },
    yAxis: {
      axisLabel: { fontSize: 10, formatter: (v: number) => formatCompact(v) },
      name: data.y_label ?? humanizeColumn(measureOf(props.node)),
      nameGap: 12,
      nameTextStyle: { fontSize: 10 },
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

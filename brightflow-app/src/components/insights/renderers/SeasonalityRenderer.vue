<script setup lang="ts">
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
  return {
    grid: { bottom: 24, containLabel: true, left: 8, right: 8, top: 24 },
    series: [
      {
        data: data.values,
        lineStyle: { color: colors[5] ?? '#14b8a6', width: 2 },
        showSymbol: false,
        smooth: true,
        type: 'line',
      },
    ],
    tooltip: { trigger: 'axis', valueFormatter: (v: number) => formatNumber(v) },
    xAxis: {
      axisLabel: { fontSize: 10, formatter: (l: string) => humanizePeriodShort(l) },
      boundaryGap: false,
      data: data.labels,
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

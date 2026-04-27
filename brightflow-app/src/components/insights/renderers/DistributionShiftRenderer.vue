<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/services/api';

import './echarts-setup';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'HistogramPair') {
    return null;
  }
  const labels = data.bin_edges.slice(0, -1).map((edge, i) => {
    const next = data.bin_edges[i + 1] ?? edge;
    return `${edge.toFixed(0)}-${next.toFixed(0)}`;
  });
  return {
    grid: { bottom: 36, containLabel: true, left: 8, right: 8, top: 8 },
    legend: {
      bottom: 0,
      itemHeight: 6,
      itemWidth: 12,
      textStyle: { fontSize: 10 },
    },
    series: [
      {
        data: data.previous,
        itemStyle: { color: `${colors[2] ?? '#94a3b8'}aa` },
        name: 'Previous',
        type: 'bar',
      },
      {
        data: data.current,
        itemStyle: { color: `${colors[3] ?? '#ef4444'}aa` },
        name: 'Current',
        type: 'bar',
      },
    ],
    tooltip: { trigger: 'axis' },
    xAxis: { axisLabel: { fontSize: 9, rotate: 30 }, data: labels, type: 'category' },
    yAxis: { axisLabel: { fontSize: 9 }, type: 'value' },
  };
});
</script>

<template>
  <div class="h-44 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No distribution data available
    </div>
  </div>
</template>

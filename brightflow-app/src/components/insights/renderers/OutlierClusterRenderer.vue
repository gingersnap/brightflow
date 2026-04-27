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
  if (!data || data.type !== 'Multi') {
    return null;
  }
  const series = data.series.map((s, i) => ({
    data: s.values,
    lineStyle: { color: colors[i % colors.length] ?? '#888', width: 1.5 },
    name: s.name,
    showSymbol: false,
    smooth: false,
    type: 'line',
  }));
  return {
    grid: { bottom: 36, containLabel: true, left: 8, right: 8, top: 16 },
    legend: {
      bottom: 0,
      itemGap: 8,
      itemHeight: 6,
      itemWidth: 12,
      textStyle: { fontSize: 10 },
    },
    series,
    tooltip: { trigger: 'axis' },
    xAxis: {
      axisLabel: { fontSize: 9 },
      boundaryGap: false,
      data: data.labels,
      type: 'category',
    },
    yAxis: { axisLabel: { fontSize: 9 }, type: 'value' },
  };
});
</script>

<template>
  <div class="h-44 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No cluster data available
    </div>
  </div>
</template>

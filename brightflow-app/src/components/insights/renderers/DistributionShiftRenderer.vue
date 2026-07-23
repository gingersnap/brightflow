<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/services/api';
import { formatCompact, formatNumber, humanizeColumn, humanizePeriod } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import './echarts-setup';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'HistogramPair') {
    return null;
  }
  const a = props.node.analysis;
  const previousName =
    a.type === 'DistributionShift' ? humanizePeriod(a.previous_period) : 'Previous';
  const currentName = a.type === 'DistributionShift' ? humanizePeriod(a.current_period) : 'Current';
  const labels = data.bin_edges.slice(0, -1).map((edge, i) => {
    const next = data.bin_edges[i + 1] ?? edge;
    return `${formatNumber(edge)}-${formatNumber(next)}`;
  });
  return {
    grid: { bottom: 52, containLabel: true, left: 8, right: 8, top: 24 },
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
        name: previousName,
        type: 'bar',
      },
      {
        data: data.current,
        itemStyle: { color: `${colors[3] ?? '#ef4444'}aa` },
        name: currentName,
        type: 'bar',
      },
    ],
    tooltip: { trigger: 'axis', valueFormatter: (v: number) => formatNumber(v) },
    xAxis: {
      axisLabel: { fontSize: 9, rotate: 30 },
      data: labels,
      name: humanizeColumn(measureOf(props.node)),
      nameGap: 32,
      nameLocation: 'middle',
      nameTextStyle: { fontSize: 10 },
      type: 'category',
    },
    yAxis: {
      axisLabel: { fontSize: 9, formatter: (v: number) => formatCompact(v) },
      name: 'Count',
      nameGap: 12,
      nameTextStyle: { fontSize: 10 },
      type: 'value',
    },
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

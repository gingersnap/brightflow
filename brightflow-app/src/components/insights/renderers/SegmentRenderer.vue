<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/services/api';

import './echarts-setup';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const segmentInfo = computed(() => {
  const a = props.node.analysis;
  if (a.type !== 'Segment') {
    return null;
  }
  return {
    column: a.segment_column,
    contributionPct: a.contribution_pct,
    value: a.segment_value,
  };
});

const chartOption = computed(() => {
  const info = segmentInfo.value;
  if (!info) {
    return null;
  }
  // Build a visual: contribution % as a horizontal bar with sign-aware color
  const value = Math.abs(info.contributionPct);
  const color = info.contributionPct >= 0 ? (colors[1] ?? '#22c55e') : (colors[3] ?? '#ef4444');
  return {
    grid: { bottom: 24, containLabel: true, left: 8, right: 24, top: 8 },
    series: [
      {
        data: [{ itemStyle: { color }, value }],
        label: {
          fontSize: 11,
          formatter: () =>
            `${info.contributionPct >= 0 ? '+' : '-'}${value.toFixed(0)}% of total change`,
          position: 'right',
          show: true,
        },
        type: 'bar',
      },
    ],
    tooltip: { show: false },
    xAxis: { axisLabel: { fontSize: 10 }, max: Math.max(value * 1.5, 10), type: 'value' },
    yAxis: {
      axisLabel: { fontSize: 11 },
      data: [`${info.column} = ${info.value}`],
      type: 'category',
    },
  };
});
</script>

<template>
  <div class="h-32 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No segment data available
    </div>
  </div>
</template>

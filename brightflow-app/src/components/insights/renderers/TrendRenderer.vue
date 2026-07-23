<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/services/api';
import { formatCompact, formatNumber, humanizeColumn, humanizePeriodShort } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import './echarts-setup';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const rSquared = computed(() => {
  const a = props.node.analysis;
  if (a.type === 'Trend') {
    return a.r_squared;
  }
  return null;
});

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'SeriesWithFit') {
    return null;
  }
  return {
    grid: { bottom: 24, containLabel: true, left: 8, right: 8, top: 24 },
    series: [
      {
        data: data.values,
        lineStyle: { color: colors[0] ?? '#3b82f6', width: 2 },
        showSymbol: false,
        smooth: false,
        type: 'line',
      },
      {
        data: data.fit,
        lineStyle: { color: colors[4] ?? '#f59e0b', type: 'dashed', width: 2 },
        showSymbol: false,
        smooth: false,
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
  <div class="relative h-40 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No chart data available
    </div>
    <div
      v-if="rSquared !== null"
      class="absolute top-2 right-2 rounded bg-elevated/80 px-1.5 py-0.5 font-mono-data text-xs text-muted"
    >
      R² = {{ rSquared.toFixed(2) }}
    </div>
  </div>
</template>

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
    grid: { bottom: 36, containLabel: true, left: 8, right: 8, top: 24 },
    legend: {
      bottom: 0,
      itemGap: 8,
      itemHeight: 6,
      itemWidth: 12,
      textStyle: { fontSize: 10 },
    },
    series,
    tooltip: { trigger: 'axis', valueFormatter: (v: number) => formatNumber(v) },
    xAxis: {
      axisLabel: { fontSize: 9, formatter: (l: string) => humanizePeriodShort(l) },
      boundaryGap: false,
      data: data.labels,
      type: 'category',
    },
    yAxis: {
      axisLabel: { fontSize: 9, formatter: (v: number) => formatCompact(v) },
      name: data.y_label ?? humanizeColumn(measureOf(props.node)),
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
      No cluster data available
    </div>
  </div>
</template>

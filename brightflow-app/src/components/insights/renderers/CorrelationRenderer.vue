<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/services/api';
import { formatCompact, formatNumber, humanizeColumn } from '@/utils/format';
import '@/services/echarts';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const r = computed(() => {
  const a = props.node.analysis;
  return a.type === 'Correlation' ? a.r_value : null;
});

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'Scatter') {
    return null;
  }
  const points = data.x.map((x, i) => [x, data.y[i] ?? 0]);
  const series: Record<string, unknown>[] = [
    {
      data: points,
      itemStyle: { color: `${colors[6] ?? '#ec4899'}cc` },
      symbolSize: 6,
      type: 'scatter',
    },
  ];
  if (data.fit_slope !== null && data.fit_intercept !== null) {
    const xs = [...data.x].toSorted((a, b) => a - b);
    const xMin = xs[0] ?? 0;
    const xMax = xs.at(-1) ?? 1;
    const slope = data.fit_slope;
    const intercept = data.fit_intercept;
    series.push({
      data: [
        [xMin, slope * xMin + intercept],
        [xMax, slope * xMax + intercept],
      ],
      lineStyle: { color: colors[4] ?? '#f59e0b', type: 'dashed', width: 2 },
      showSymbol: false,
      type: 'line',
    });
  }
  const xName = humanizeColumn(data.x_label);
  const yName = humanizeColumn(data.y_label);
  return {
    grid: { bottom: 30, containLabel: true, left: 8, right: 8, top: 24 },
    series,
    tooltip: {
      formatter: (p: { value: [number, number] }) =>
        `${xName}: ${formatNumber(p.value[0])}<br/>${yName}: ${formatNumber(p.value[1])}`,
      trigger: 'item',
    },
    xAxis: {
      axisLabel: { fontSize: 10, formatter: (v: number) => formatCompact(v) },
      name: xName,
      nameTextStyle: { fontSize: 10 },
      scale: true,
      type: 'value',
    },
    yAxis: {
      axisLabel: { fontSize: 10, formatter: (v: number) => formatCompact(v) },
      name: yName,
      nameGap: 12,
      nameTextStyle: { fontSize: 10 },
      scale: true,
      type: 'value',
    },
  };
});
</script>

<template>
  <div class="relative h-40 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No correlation data available
    </div>
    <div
      v-if="r !== null"
      class="absolute top-2 right-2 rounded bg-elevated/80 px-1.5 py-0.5 font-mono-data text-xs text-muted"
    >
      r = {{ r.toFixed(2) }}
    </div>
  </div>
</template>

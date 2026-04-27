<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/services/api';

import './echarts-setup';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const gini = computed(() => {
  const a = props.node.analysis;
  return a.type === 'Concentration' ? a.hhi : null;
});

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'Lorenz') {
    return null;
  }
  // Lorenz curve: cumulative_population (X) vs cumulative_share (Y)
  const lorenz = data.cumulative_population.map((p, i) => [
    p * 100,
    (data.cumulative_share[i] ?? 0) * 100,
  ]);
  // Reference line: equal distribution (perfect equality)
  const equality = [
    [0, 0],
    [100, 100],
  ];
  return {
    grid: { bottom: 30, containLabel: true, left: 8, right: 8, top: 8 },
    series: [
      {
        data: equality,
        lineStyle: { color: colors[2] ?? '#94a3b8', type: 'dashed', width: 1 },
        showSymbol: false,
        type: 'line',
      },
      {
        areaStyle: { color: `${colors[7] ?? '#a855f7'}33` },
        data: lorenz,
        lineStyle: { color: colors[7] ?? '#a855f7', width: 2 },
        showSymbol: false,
        type: 'line',
      },
    ],
    tooltip: {
      formatter: (p: { value: [number, number] }) =>
        `Bottom ${p.value[0].toFixed(0)}% has ${p.value[1].toFixed(0)}% of total`,
      trigger: 'item',
    },
    xAxis: {
      axisLabel: { fontSize: 10, formatter: '{value}%' },
      max: 100,
      name: 'Population',
      nameGap: 18,
      nameLocation: 'middle',
      type: 'value',
    },
    yAxis: {
      axisLabel: { fontSize: 10, formatter: '{value}%' },
      max: 100,
      name: 'Share',
      type: 'value',
    },
  };
});
</script>

<template>
  <div class="relative h-44 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No concentration data available
    </div>
    <div
      v-if="gini !== null"
      class="absolute top-2 right-2 rounded bg-elevated/80 px-1.5 py-0.5 font-mono-data text-xs text-muted"
    >
      HHI = {{ gini.toFixed(2) }}
    </div>
  </div>
</template>

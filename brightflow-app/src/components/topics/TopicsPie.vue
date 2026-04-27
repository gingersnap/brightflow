<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

import type { ClusterSummary } from '@/types/generated';

import './echarts-setup';
import { clusterColor } from './colors';

const props = defineProps<{
  clusters: ClusterSummary[];
}>();

const chartOption = computed(() => {
  const data = props.clusters.map((c, idx) => ({
    name: (c.topTerms[0] ?? c.name).slice(0, 40),
    value: c.size,
    itemStyle: { color: clusterColor(idx) },
  }));
  return {
    tooltip: {
      trigger: 'item',
      formatter: '{b}: {c} ({d}%)',
    },
    legend: { show: false },
    series: [
      {
        type: 'pie',
        radius: ['45%', '75%'],
        center: ['50%', '50%'],
        avoidLabelOverlap: true,
        label: {
          show: true,
          formatter: '{d}%',
          fontSize: 11,
          color: '#9ca3af',
        },
        labelLine: { show: true, length: 6, length2: 4 },
        emphasis: {
          label: { show: true, fontSize: 12, fontWeight: 'bold' },
        },
        data,
      },
    ],
  };
});
</script>

<template>
  <VChart v-if="clusters.length > 0" :option="chartOption" autoresize class="h-full w-full" />
</template>

<script setup lang="ts">
/**
 * Smoothed line over the period series: easing the curve (smooth: true)
 * emphasizes the repeating rise-and-fall shape of a seasonal cycle over the
 * individual point values.
 */

import { computed } from 'vue';
import VChart from 'vue-echarts';

import EmptyState from '@/components/common/EmptyState.vue';
import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/types/generated';
import { humanizeColumn, humanizePeriodShort } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import { periodSeriesOption } from './periodSeriesOption';
import '@/services/echarts';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'Series') {
    return null;
  }
  return periodSeriesOption({
    boundaryGap: false,
    labels: data.labels,
    series: [
      {
        data: data.values,
        lineStyle: { color: colors[5] ?? '#14b8a6', width: 2 },
        showSymbol: false,
        smooth: true,
        type: 'line',
      },
    ],
    xFormatter: (l: string) => humanizePeriodShort(l),
    yName: data.y_label ?? humanizeColumn(measureOf(props.node)),
  });
});
</script>

<template>
  <div class="h-40 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <EmptyState v-else class="h-full" message="No chart data available" />
  </div>
</template>

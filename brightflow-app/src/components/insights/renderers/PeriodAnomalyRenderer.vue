<script setup lang="ts">
/**
 * One bar per period, with the flagged period singled out purely by color —
 * red at the marker index, a single palette color everywhere else — so the
 * whole visual is the flagged period's height against its ordinary
 * neighbors.
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
  const labels = data.labels;
  const values = data.values;
  const marker = data.marker_index;

  const itemColor = (i: number): string => {
    if (i === marker) {
      return '#ef4444';
    }
    return colors[2] ?? '#94a3b8';
  };
  return periodSeriesOption({
    gridBottom: 30,
    labels,
    rotate: 30,
    series: [
      {
        data: values.map((v, i) => ({ itemStyle: { color: itemColor(i) }, value: v })),
        type: 'bar',
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

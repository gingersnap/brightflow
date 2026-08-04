<script setup lang="ts">
/**
 * Exactly two bars — previous period and current — in a single palette
 * color, with humanized period names on the axis. The size gap between the
 * bars is the entire visualization; nothing else is drawn.
 */

import { computed } from 'vue';
import VChart from 'vue-echarts';

import EmptyState from '@/components/common/EmptyState.vue';
import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/types/generated';
import { humanizeColumn, humanizePeriod } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import { periodSeriesOption } from './periodSeriesOption';
import '@/services/echarts';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'PairedBars') {
    return null;
  }
  return periodSeriesOption({
    gridBottom: 30,
    labels: data.labels.map((l) => humanizePeriod(l)),
    series: [
      {
        data: [data.previous[0] ?? 0, data.current[0] ?? 0],
        itemStyle: { color: colors[2] ?? '#94a3b8' },
        type: 'bar',
      },
    ],
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

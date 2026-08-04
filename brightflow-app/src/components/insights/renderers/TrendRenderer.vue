<script setup lang="ts">
/**
 * Actual series as a solid line with the linear fit dashed over it, and R²
 * pinned as a corner chip — how closely the two lines track each other shows
 * how much of the movement the fitted trend explains.
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
  return periodSeriesOption({
    boundaryGap: false,
    labels: data.labels,
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
    xFormatter: (l: string) => humanizePeriodShort(l),
    yName: data.y_label ?? humanizeColumn(measureOf(props.node)),
  });
});
</script>

<template>
  <div class="relative h-40 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <EmptyState v-else class="h-full" message="No chart data available" />
    <div
      v-if="rSquared !== null"
      class="absolute top-2 right-2 rounded bg-elevated/80 px-1.5 py-0.5 font-mono-data text-xs text-muted"
    >
      R² = {{ rSquared.toFixed(2) }}
    </div>
  </div>
</template>

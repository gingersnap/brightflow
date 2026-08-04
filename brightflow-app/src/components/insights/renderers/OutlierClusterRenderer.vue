<script setup lang="ts">
/**
 * Overlaid line chart for outlier-cluster findings: every series in the
 * Multi payload gets its own thin line on shared axes with a bottom legend,
 * each in its own palette color at the same weight — no member is
 * pre-highlighted, so the deviating one has to show against the pack.
 */

import { computed } from 'vue';
import VChart from 'vue-echarts';

import EmptyState from '@/components/common/EmptyState.vue';
import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/types/generated';
import { formatCompact, humanizeColumn, humanizePeriodShort } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import { periodSeriesOption } from './periodSeriesOption';
import '@/services/echarts';

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
  return periodSeriesOption({
    boundaryGap: false,
    gridBottom: 36,
    labels: data.labels,
    legend: true,
    series,
    xAxis: { axisLabel: { fontSize: 9, formatter: (l: string) => humanizePeriodShort(l) } },
    yAxis: { axisLabel: { fontSize: 9, formatter: (v: number) => formatCompact(v) } },
    yName: data.y_label ?? humanizeColumn(measureOf(props.node)),
  });
});
</script>

<template>
  <div class="h-44 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <EmptyState v-else class="h-full" message="No cluster data available" />
  </div>
</template>

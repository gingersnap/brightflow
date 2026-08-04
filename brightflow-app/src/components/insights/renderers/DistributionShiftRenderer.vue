<script setup lang="ts">
/**
 * Grouped histogram pair over shared bin edges — previous and current period
 * in two palette colors — so a shift reads as mass sliding between the same
 * buckets. Legend names are the analysis' actual periods, humanized, falling
 * back to generic "Previous"/"Current" only when those are missing.
 */

import { computed } from 'vue';
import VChart from 'vue-echarts';

import EmptyState from '@/components/common/EmptyState.vue';
import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/types/generated';
import { formatCompact, formatNumber, humanizeColumn, humanizePeriod } from '@/utils/format';

import { measureOf } from '../nodeMeta';
import { periodSeriesOption } from './periodSeriesOption';
import '@/services/echarts';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const chartOption = computed(() => {
  const data = props.node.data;
  if (!data || data.type !== 'HistogramPair') {
    return null;
  }
  const a = props.node.analysis;
  const previousName =
    a.type === 'DistributionShift' ? humanizePeriod(a.previous_period) : 'Previous';
  const currentName = a.type === 'DistributionShift' ? humanizePeriod(a.current_period) : 'Current';
  const labels = data.bin_edges.slice(0, -1).map((edge, i) => {
    const next = data.bin_edges[i + 1] ?? edge;
    return `${formatNumber(edge)}-${formatNumber(next)}`;
  });
  return periodSeriesOption({
    gridBottom: 52,
    labels,
    legend: true,
    series: [
      {
        data: data.previous,
        itemStyle: { color: `${colors[2] ?? '#94a3b8'}aa` },
        name: previousName,
        type: 'bar',
      },
      {
        data: data.current,
        itemStyle: { color: `${colors[3] ?? '#ef4444'}aa` },
        name: currentName,
        type: 'bar',
      },
    ],
    xAxis: {
      axisLabel: { fontSize: 9, rotate: 30 },
      name: humanizeColumn(measureOf(props.node)),
      nameGap: 32,
      nameLocation: 'middle',
      nameTextStyle: { fontSize: 10 },
    },
    yAxis: { axisLabel: { fontSize: 9, formatter: (v: number) => formatCompact(v) } },
    yName: 'Count',
  });
});
</script>

<template>
  <div class="h-44 w-full">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <EmptyState v-else class="h-full" message="No distribution data available" />
  </div>
</template>

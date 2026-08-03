<script setup lang="ts">
/**
 * Horizontal bar chart of segment contributions, colored by sign (green
 * positive, red negative) with each bar's share of the total movement in
 * the tooltip. When the node carries no SegmentBars payload it falls back
 * to a single contribution bar synthesized from the Segment analysis
 * fields, labeled "±N% of total change".
 */

import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import type { AnalysisNode } from '@/types/generated';
import {
  displaySegmentValue,
  formatCompact,
  formatNumber,
  formatPercent,
  humanizeColumn,
} from '@/utils/format';

import { measureOf } from '../nodeMeta';
import '@/services/echarts';

const props = defineProps<{ node: AnalysisNode }>();
const colors = useChartColors();

const segmentBars = computed(() => {
  const data = props.node.data;
  return data?.type === 'SegmentBars' ? data : null;
});

const segmentInfo = computed(() => {
  const a = props.node.analysis;
  if (a.type !== 'Segment') {
    return null;
  }
  return {
    column: a.segment_column,
    contributionPct: a.contribution_pct,
    value: a.segment_value,
  };
});

const chartOption = computed(() => {
  const bars = segmentBars.value;
  if (bars) {
    const positive = colors[1] ?? '#22c55e';
    const negative = colors[3] ?? '#ef4444';
    const valueLabel = bars.value_label ?? humanizeColumn(measureOf(props.node));
    return {
      grid: { bottom: 30, containLabel: true, left: 8, right: 24, top: 8 },
      series: [
        {
          data: bars.values.map((v) => ({
            itemStyle: { color: v >= 0 ? positive : negative },
            value: v,
          })),
          type: 'bar',
        },
      ],
      tooltip: {
        formatter: (p: { dataIndex: number; name: string; value: number }) => {
          const pct = bars.contributions_pct[p.dataIndex];
          const pctLine = pct === undefined ? '' : `<br/>${formatPercent(pct)} of the movement`;
          return `${p.name}: ${formatNumber(p.value)}${pctLine}`;
        },
        trigger: 'item',
      },
      xAxis: {
        axisLabel: { fontSize: 10, formatter: (v: number) => formatCompact(v) },
        name: valueLabel,
        nameGap: 18,
        nameLocation: 'middle',
        nameTextStyle: { fontSize: 10 },
        type: 'value',
      },
      yAxis: {
        axisLabel: { fontSize: 11 },
        data: bars.labels.map((l) => displaySegmentValue(l)),
        inverse: true,
        type: 'category',
      },
    };
  }
  const info = segmentInfo.value;
  if (!info) {
    return null;
  }
  // Build a visual: contribution % as a horizontal bar with sign-aware color
  const value = Math.abs(info.contributionPct);
  const color = info.contributionPct >= 0 ? (colors[1] ?? '#22c55e') : (colors[3] ?? '#ef4444');
  return {
    grid: { bottom: 24, containLabel: true, left: 8, right: 24, top: 8 },
    series: [
      {
        data: [{ itemStyle: { color }, value }],
        label: {
          fontSize: 11,
          formatter: () =>
            `${info.contributionPct >= 0 ? '+' : '-'}${value.toFixed(0)}% of total change`,
          position: 'right',
          show: true,
        },
        type: 'bar',
      },
    ],
    tooltip: { show: false },
    xAxis: { axisLabel: { fontSize: 10 }, max: Math.max(value * 1.5, 10), type: 'value' },
    yAxis: {
      axisLabel: { fontSize: 11 },
      data: [`${info.column} = ${info.value}`],
      type: 'category',
    },
  };
});
</script>

<template>
  <div class="w-full" :class="segmentBars ? 'h-40' : 'h-32'">
    <VChart v-if="chartOption" :option="chartOption" autoresize class="h-full w-full" />
    <div v-else class="flex h-full items-center justify-center text-sm text-muted">
      No segment data available
    </div>
  </div>
</template>

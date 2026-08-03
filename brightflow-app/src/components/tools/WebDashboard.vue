<script setup lang="ts">
/**
 * Overview dashboard for a web-analytics source: stat tiles, a visitors and
 * pageviews time-series, and four breakdown tables. Each block gets its own
 * query keyed on source + period, so the sections load and cache
 * independently when the period changes.
 */

import { useQuery } from '@pinia/colada';
import { LineChart, BarChart } from 'echarts/charts';

import '@/services/echarts';
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { useChartColors } from '@/composables/useChartColors';
import { analyticsApi } from '@/services/api';
import type { TimeseriesPoint } from '@/types';

const colors = useChartColors();

const props = defineProps<{
  sourceId: string;
  period: string;
}>();

const { data: stats } = useQuery({
  key: () => ['analytics-stats', props.sourceId, props.period],
  query: async () => await analyticsApi.stats(props.sourceId, props.period),
});

const { data: timeseries } = useQuery({
  key: () => ['analytics-timeseries', props.sourceId, props.period],
  query: async () => await analyticsApi.timeseries(props.sourceId, props.period),
});

const { data: topPages } = useQuery({
  key: () => ['analytics-top-pages', props.sourceId, props.period],
  query: async () => await analyticsApi.topPages(props.sourceId, props.period),
});

const { data: referrers } = useQuery({
  key: () => ['analytics-referrers', props.sourceId, props.period],
  query: async () => await analyticsApi.referrers(props.sourceId, props.period),
});

const { data: devices } = useQuery({
  key: () => ['analytics-devices', props.sourceId, props.period],
  query: async () => await analyticsApi.devices(props.sourceId, props.period),
});

const { data: geoData } = useQuery({
  key: () => ['analytics-geo', props.sourceId, props.period],
  query: async () => await analyticsApi.geo(props.sourceId, props.period),
});

const chartOption = computed(() => {
  const data = timeseries.value ?? [];
  return {
    color: colors,
    tooltip: { trigger: 'axis' },
    grid: { left: 50, right: 20, top: 20, bottom: 30 },
    xAxis: {
      type: 'category',
      data: data.map((d: TimeseriesPoint) => d.date),
    },
    yAxis: { type: 'value' },
    series: [
      {
        name: 'Visitors',
        type: 'line',
        smooth: true,
        data: data.map((d: TimeseriesPoint) => d.visitors),
        areaStyle: { opacity: 0.15 },
      },
      {
        name: 'Pageviews',
        type: 'line',
        smooth: true,
        data: data.map((d: TimeseriesPoint) => d.pageviews),
        areaStyle: { opacity: 0.1 },
      },
    ],
  };
});
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-4">
    <!-- Stats bar -->
    <div class="mb-4 grid grid-cols-4 gap-3">
      <div class="rounded-lg border border-default bg-elevated p-4">
        <p class="text-sm text-muted">Unique Visitors</p>
        <p class="mt-1 text-2xl font-semibold text-highlighted">
          {{ stats?.visitors?.toLocaleString() ?? '-' }}
        </p>
      </div>
      <div class="rounded-lg border border-default bg-elevated p-4">
        <p class="text-sm text-muted">Total Pageviews</p>
        <p class="mt-1 text-2xl font-semibold text-highlighted">
          {{ stats?.pageviews?.toLocaleString() ?? '-' }}
        </p>
      </div>
      <div class="rounded-lg border border-default bg-elevated p-4">
        <p class="text-sm text-muted">Bounce Rate</p>
        <p class="mt-1 text-2xl font-semibold text-highlighted">
          {{ stats?.bounceRate != null ? `${(stats.bounceRate * 100).toFixed(1)}%` : '-' }}
        </p>
      </div>
      <div class="rounded-lg border border-default bg-elevated p-4">
        <p class="text-sm text-muted">Avg Visit Duration</p>
        <p class="mt-1 text-2xl font-semibold text-highlighted">
          {{ stats?.avgVisitDuration != null ? `${stats.avgVisitDuration.toFixed(0)}s` : '-' }}
        </p>
      </div>
    </div>

    <!-- Visitors chart -->
    <div class="mb-4 rounded-lg border border-default bg-elevated p-4">
      <h3 class="mb-3 text-sm font-medium text-highlighted">Visitors & Pageviews</h3>
      <v-chart
        v-if="timeseries && timeseries.length > 0"
        :option="chartOption"
        style="height: 250px"
        autoresize
      />
      <p v-else class="py-12 text-center text-sm text-muted">No data for this period</p>
    </div>

    <!-- Breakdowns grid -->
    <div class="grid grid-cols-2 gap-3">
      <BreakdownTable title="Top Pages" name-label="Page" :rows="topPages" empty-name="/" />
      <BreakdownTable title="Sources" name-label="Source" :rows="referrers" />
      <BreakdownTable title="Browsers" name-label="Browser" :rows="devices" />
      <BreakdownTable title="Countries" name-label="Country" :rows="geoData" empty-name="Unknown" />
    </div>
  </div>
</template>

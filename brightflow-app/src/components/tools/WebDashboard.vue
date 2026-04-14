<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { LineChart, BarChart } from 'echarts/charts';
import { GridComponent, TooltipComponent, LegendComponent } from 'echarts/components';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { computed } from 'vue';
import VChart from 'vue-echarts';

import { analyticsApi } from '@/services/api';
import type { TimeseriesPoint } from '@/types';

use([CanvasRenderer, LineChart, BarChart, GridComponent, TooltipComponent, LegendComponent]);

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
        <p class="text-xs text-muted">Unique Visitors</p>
        <p class="mt-1 text-2xl font-semibold text-highlighted">
          {{ stats?.visitors?.toLocaleString() ?? '-' }}
        </p>
      </div>
      <div class="rounded-lg border border-default bg-elevated p-4">
        <p class="text-xs text-muted">Total Pageviews</p>
        <p class="mt-1 text-2xl font-semibold text-highlighted">
          {{ stats?.pageviews?.toLocaleString() ?? '-' }}
        </p>
      </div>
      <div class="rounded-lg border border-default bg-elevated p-4">
        <p class="text-xs text-muted">Bounce Rate</p>
        <p class="mt-1 text-2xl font-semibold text-highlighted">
          {{ stats?.bounceRate != null ? `${(stats.bounceRate * 100).toFixed(1)}%` : '-' }}
        </p>
      </div>
      <div class="rounded-lg border border-default bg-elevated p-4">
        <p class="text-xs text-muted">Avg Visit Duration</p>
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
      <!-- Top Pages -->
      <div class="rounded-lg border border-default bg-elevated p-4">
        <h3 class="mb-3 text-sm font-medium text-highlighted">Top Pages</h3>
        <table v-if="topPages && topPages.length > 0" class="w-full text-sm">
          <thead>
            <tr class="border-b border-default text-xs text-muted">
              <th class="pb-2 text-left font-medium">Page</th>
              <th class="pb-2 text-right font-medium">Visitors</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in topPages"
              :key="row.name"
              class="border-b border-default last:border-0"
            >
              <td class="py-1.5 text-highlighted">{{ row.name || '/' }}</td>
              <td class="py-1.5 text-right text-muted">{{ row.visitors.toLocaleString() }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="py-4 text-center text-xs text-muted">No data</p>
      </div>

      <!-- Referrers -->
      <div class="rounded-lg border border-default bg-elevated p-4">
        <h3 class="mb-3 text-sm font-medium text-highlighted">Sources</h3>
        <table v-if="referrers && referrers.length > 0" class="w-full text-sm">
          <thead>
            <tr class="border-b border-default text-xs text-muted">
              <th class="pb-2 text-left font-medium">Source</th>
              <th class="pb-2 text-right font-medium">Visitors</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in referrers"
              :key="row.name"
              class="border-b border-default last:border-0"
            >
              <td class="py-1.5 text-highlighted">{{ row.name }}</td>
              <td class="py-1.5 text-right text-muted">{{ row.visitors.toLocaleString() }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="py-4 text-center text-xs text-muted">No data</p>
      </div>

      <!-- Browsers -->
      <div class="rounded-lg border border-default bg-elevated p-4">
        <h3 class="mb-3 text-sm font-medium text-highlighted">Browsers</h3>
        <table v-if="devices && devices.length > 0" class="w-full text-sm">
          <thead>
            <tr class="border-b border-default text-xs text-muted">
              <th class="pb-2 text-left font-medium">Browser</th>
              <th class="pb-2 text-right font-medium">Visitors</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in devices"
              :key="row.name"
              class="border-b border-default last:border-0"
            >
              <td class="py-1.5 text-highlighted">{{ row.name }}</td>
              <td class="py-1.5 text-right text-muted">{{ row.visitors.toLocaleString() }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="py-4 text-center text-xs text-muted">No data</p>
      </div>

      <!-- Countries -->
      <div class="rounded-lg border border-default bg-elevated p-4">
        <h3 class="mb-3 text-sm font-medium text-highlighted">Countries</h3>
        <table v-if="geoData && geoData.length > 0" class="w-full text-sm">
          <thead>
            <tr class="border-b border-default text-xs text-muted">
              <th class="pb-2 text-left font-medium">Country</th>
              <th class="pb-2 text-right font-medium">Visitors</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in geoData"
              :key="row.name"
              class="border-b border-default last:border-0"
            >
              <td class="py-1.5 text-highlighted">{{ row.name || 'Unknown' }}</td>
              <td class="py-1.5 text-right text-muted">{{ row.visitors.toLocaleString() }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="py-4 text-center text-xs text-muted">No data</p>
      </div>
    </div>
  </div>
</template>

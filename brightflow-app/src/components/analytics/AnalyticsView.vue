<script setup lang="ts">
import { useQuery, useMutation } from '@pinia/colada';
import { LineChart, BarChart } from 'echarts/charts';
import { GridComponent, TooltipComponent, LegendComponent } from 'echarts/components';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { BarChart3, Copy, Globe, Plus, Trash2 } from 'lucide-vue-next';
import { ref, computed, watch } from 'vue';
import VChart from 'vue-echarts';

import { sourceApi, analyticsApi } from '@/services/api';
import type { Source, DashboardStats, TimeseriesPoint, BreakdownRow } from '@/types';

use([CanvasRenderer, LineChart, BarChart, GridComponent, TooltipComponent, LegendComponent]);

const selectedSourceId = ref<string | null>(null);
const period = ref('30d');
const showAddSource = ref(false);
const newDomain = ref('');
const newName = ref('');

// Sources list
const { data: sources, refresh: refreshSources } = useQuery({
  key: ['sources'],
  query: async () => {
    const result = await sourceApi.list();
    return result ?? [];
  },
});

// Auto-select first source
watch(sources, (val) => {
  if (val && val.length > 0 && !selectedSourceId.value) {
    selectedSourceId.value = val[0]!.id;
  }
});

const selectedSource = computed(() =>
  (sources.value ?? []).find((s) => s.id === selectedSourceId.value),
);

// Dashboard data
const { data: stats } = useQuery({
  key: () => ['analytics-stats', selectedSourceId.value, period.value],
  query: async () => {
    if (!selectedSourceId.value) {
      return null;
    }
    return await analyticsApi.stats(selectedSourceId.value, period.value);
  },
  enabled: () => Boolean(selectedSourceId.value),
});

const { data: timeseries } = useQuery({
  key: () => ['analytics-timeseries', selectedSourceId.value, period.value],
  query: async () => {
    if (!selectedSourceId.value) {
      return null;
    }
    return await analyticsApi.timeseries(selectedSourceId.value, period.value);
  },
  enabled: () => Boolean(selectedSourceId.value),
});

const { data: topPages } = useQuery({
  key: () => ['analytics-top-pages', selectedSourceId.value, period.value],
  query: async () => {
    if (!selectedSourceId.value) {
      return null;
    }
    return await analyticsApi.topPages(selectedSourceId.value, period.value);
  },
  enabled: () => Boolean(selectedSourceId.value),
});

const { data: referrers } = useQuery({
  key: () => ['analytics-referrers', selectedSourceId.value, period.value],
  query: async () => {
    if (!selectedSourceId.value) {
      return null;
    }
    return await analyticsApi.referrers(selectedSourceId.value, period.value);
  },
  enabled: () => Boolean(selectedSourceId.value),
});

const { data: devices } = useQuery({
  key: () => ['analytics-devices', selectedSourceId.value, period.value],
  query: async () => {
    if (!selectedSourceId.value) {
      return null;
    }
    return await analyticsApi.devices(selectedSourceId.value, period.value);
  },
  enabled: () => Boolean(selectedSourceId.value),
});

const { data: geoData } = useQuery({
  key: () => ['analytics-geo', selectedSourceId.value, period.value],
  query: async () => {
    if (!selectedSourceId.value) {
      return null;
    }
    return await analyticsApi.geo(selectedSourceId.value, period.value);
  },
  enabled: () => Boolean(selectedSourceId.value),
});

// Add source mutation
const { mutate: addSource } = useMutation({
  mutation: async () => {
    await sourceApi.create(newDomain.value, newName.value || newDomain.value);
    newDomain.value = '';
    newName.value = '';
    showAddSource.value = false;
    await refreshSources();
  },
});

// Delete source mutation
const { mutate: deleteSource } = useMutation({
  mutation: async (id: string) => {
    await sourceApi.delete(id);
    if (selectedSourceId.value === id) {
      selectedSourceId.value = null;
    }
    await refreshSources();
  },
});

// Snippet
const snippetText = ref('');
async function loadSnippet(id: string): Promise<void> {
  const result = await sourceApi.snippet(id);
  snippetText.value = result?.snippet ?? '';
}

function copySnippet(): void {
  navigator.clipboard.writeText(snippetText.value);
}

// Chart options
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

const periods = [
  { label: 'Today', value: 'today' },
  { label: '7 days', value: '7d' },
  { label: '30 days', value: '30d' },
  { label: 'This month', value: 'month' },
  { label: '12 months', value: '12m' },
];
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-6">
    <!-- Header: Source selector + Period -->
    <div class="mb-6 flex items-center justify-between">
      <div class="flex items-center gap-3">
        <BarChart3 class="h-5 w-5 text-primary-500" />
        <h2 class="text-lg font-semibold text-highlighted">Web Analytics</h2>

        <!-- Source selector -->
        <select
          v-if="sources && sources.length > 0"
          v-model="selectedSourceId"
          class="ml-4 rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
        >
          <option v-for="s in sources" :key="s.id" :value="s.id">
            {{ s.domain }}
          </option>
        </select>

        <UButton size="xs" variant="ghost" @click="showAddSource = !showAddSource">
          <Plus class="h-4 w-4" />
        </UButton>
      </div>

      <!-- Period selector -->
      <div class="flex items-center gap-1 rounded-lg bg-elevated p-0.5">
        <button
          v-for="p in periods"
          :key="p.value"
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors"
          :class="
            period === p.value
              ? 'bg-default text-highlighted shadow-sm'
              : 'cursor-pointer text-muted hover:text-highlighted'
          "
          @click="period = p.value"
        >
          {{ p.label }}
        </button>
      </div>
    </div>

    <!-- Add source form -->
    <div v-if="showAddSource" class="mb-6 rounded-lg border border-default bg-elevated p-4">
      <h3 class="mb-3 text-sm font-medium text-highlighted">Add Website</h3>
      <div class="flex items-end gap-3">
        <div>
          <label class="mb-1 block text-xs text-muted">Domain</label>
          <input
            v-model="newDomain"
            type="text"
            placeholder="example.com"
            class="rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs text-muted">Name (optional)</label>
          <input
            v-model="newName"
            type="text"
            placeholder="My Website"
            class="rounded-lg border border-default bg-default px-3 py-1.5 text-sm"
          />
        </div>
        <UButton size="sm" @click="addSource()">Add</UButton>
      </div>
    </div>

    <!-- Tracking snippet -->
    <div v-if="selectedSource && !snippetText" class="mb-6">
      <button class="text-xs text-primary-500 underline" @click="loadSnippet(selectedSource.id)">
        Show tracking snippet
      </button>
    </div>
    <div v-if="snippetText" class="mb-6 rounded-lg border border-default bg-elevated p-4">
      <div class="mb-2 flex items-center justify-between">
        <span class="text-xs font-medium text-muted">Add this to your website's &lt;head&gt;</span>
        <UButton size="xs" variant="ghost" @click="copySnippet">
          <Copy class="h-3.5 w-3.5" />
        </UButton>
      </div>
      <code class="block rounded bg-default p-2 text-xs text-highlighted">{{ snippetText }}</code>
    </div>

    <!-- Empty state -->
    <div
      v-if="!sources || sources.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-4 text-muted"
    >
      <Globe class="h-12 w-12 opacity-40" />
      <p>No websites added yet</p>
      <UButton @click="showAddSource = true">Add your first website</UButton>
    </div>

    <!-- Dashboard -->
    <template v-else-if="selectedSourceId">
      <!-- Stats bar -->
      <div class="mb-6 grid grid-cols-4 gap-4">
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
      <div class="mb-6 rounded-lg border border-default bg-elevated p-4">
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
      <div class="grid grid-cols-2 gap-6">
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

      <!-- Source management: delete -->
      <div v-if="selectedSource" class="mt-8 border-t border-default pt-4">
        <UButton variant="ghost" color="error" size="xs" @click="deleteSource(selectedSource.id)">
          <Trash2 class="h-3.5 w-3.5" />
          Delete {{ selectedSource.domain }}
        </UButton>
      </div>
    </template>
  </div>
</template>

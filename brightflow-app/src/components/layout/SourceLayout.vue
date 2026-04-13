<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { computed } from 'vue';

import EventListPanel from '@/components/analytics/EventListPanel.vue';
import FunnelPanel from '@/components/analytics/FunnelPanel.vue';
import RetentionPanel from '@/components/analytics/RetentionPanel.vue';
import UserExplorerPanel from '@/components/analytics/UserExplorerPanel.vue';
import ExploreTool from '@/components/explore/ExploreTool.vue';
import InsightsView from '@/components/insights/InsightsView.vue';
import AppSidebar from '@/components/layout/AppSidebar.vue';
import PeriodSelector from '@/components/layout/PeriodSelector.vue';
import ConnectorDashboard from '@/components/tools/ConnectorDashboard.vue';
import WebDashboard from '@/components/tools/WebDashboard.vue';
import { productAnalyticsApi } from '@/services/api';
import { useSourceStore } from '@/stores/source';

const sourceStore = useSourceStore();

const showPeriod = computed(
  () =>
    sourceStore.selectedSource?.kind === 'web-analytics' &&
    ['dashboard', 'funnels', 'retention'].includes(sourceStore.selectedTool),
);

// Extract the raw event source ID (strip "web:" prefix) for analytics API calls
const eventSourceId = computed(() => {
  const id = sourceStore.selectedSource?.id ?? '';
  return id.startsWith('web:') ? id.slice(4) : id;
});

// Fetch event names for funnel/retention when on a web-analytics source
const { data: eventList } = useQuery({
  key: () => ['pa-event-names', eventSourceId.value, sourceStore.period],
  query: async () => await productAnalyticsApi.events(eventSourceId.value, sourceStore.period),
  enabled: () =>
    sourceStore.selectedSource?.kind === 'web-analytics' &&
    ['funnels', 'retention', 'events'].includes(sourceStore.selectedTool),
});

const eventNames = computed(() => (eventList.value ?? []).map((e) => e.name));
</script>

<template>
  <div class="flex h-full">
    <AppSidebar />
    <div class="flex min-w-0 flex-1 flex-col">
      <!-- Period selector toolbar -->
      <div v-if="showPeriod" class="flex items-center border-b border-default px-6 py-2">
        <PeriodSelector v-model="sourceStore.period" />
      </div>

      <div class="min-h-0 flex-1">
        <!-- Dashboard -->
        <template v-if="sourceStore.selectedTool === 'dashboard'">
          <WebDashboard
            v-if="sourceStore.selectedSource?.kind === 'web-analytics'"
            :source-id="eventSourceId"
            :period="sourceStore.period"
          />
          <ConnectorDashboard
            v-else-if="sourceStore.selectedSource"
            :source="sourceStore.selectedSource"
          />
        </template>

        <!-- Funnels -->
        <div v-else-if="sourceStore.selectedTool === 'funnels'" class="h-full overflow-y-auto p-6">
          <div class="rounded-lg border border-default bg-elevated p-4">
            <h3 class="mb-4 text-sm font-medium text-highlighted">Funnels</h3>
            <FunnelPanel
              :source-id="eventSourceId"
              :period="sourceStore.period"
              :event-names="eventNames"
            />
          </div>
        </div>

        <!-- Retention -->
        <div
          v-else-if="sourceStore.selectedTool === 'retention'"
          class="h-full overflow-y-auto p-6"
        >
          <div class="rounded-lg border border-default bg-elevated p-4">
            <h3 class="mb-4 text-sm font-medium text-highlighted">Retention</h3>
            <RetentionPanel
              :source-id="eventSourceId"
              :period="sourceStore.period"
              :event-names="eventNames"
            />
          </div>
        </div>

        <!-- Users -->
        <div v-else-if="sourceStore.selectedTool === 'users'" class="h-full overflow-y-auto p-6">
          <div class="rounded-lg border border-default bg-elevated p-4">
            <h3 class="mb-4 text-sm font-medium text-highlighted">Users</h3>
            <UserExplorerPanel :source-id="eventSourceId" />
          </div>
        </div>

        <!-- Explore -->
        <ExploreTool v-else-if="sourceStore.selectedTool === 'explore'" />

        <!-- Insights -->
        <div v-else-if="sourceStore.selectedTool === 'insights'" class="h-full overflow-hidden">
          <InsightsView />
        </div>
      </div>
    </div>
  </div>
</template>

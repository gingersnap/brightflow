<script setup lang="ts">
/**
 * Per-source route target: switches the :tool param across all tool
 * panels inside one shared navbar + period toolbar. Event names for the
 * funnel/retention/events tools are fetched here via Pinia Colada, keyed
 * on source and period, so those panels share one cached list; the
 * "web:" prefix is stripped off the source id for analytics API calls.
 */

import { useQuery } from '@pinia/colada';
import { computed, defineAsyncComponent } from 'vue';

import PeriodSelector from '@/components/layout/PeriodSelector.vue';
import { useSources } from '@/composables/useSources';
import { productAnalyticsApi } from '@/services/api';
import { useSourceStore } from '@/stores/source';
import { TOOL_DEFS, type ToolId } from '@/types';

/*
 * Tools load on demand: the router's lazy-loading stops at this layout, so a
 * static import here would pull every tool (echarts included) into the first
 * chunk. Each tool becomes its own chunk and loads when its tab is opened.
 */
const WebDashboard = defineAsyncComponent(() => import('@/components/tools/WebDashboard.vue'));
const ConnectorDashboard = defineAsyncComponent(
  () => import('@/components/tools/ConnectorDashboard.vue'),
);
const FunnelPanel = defineAsyncComponent(() => import('@/components/analytics/FunnelPanel.vue'));
const RetentionPanel = defineAsyncComponent(
  () => import('@/components/analytics/RetentionPanel.vue'),
);
const UserExplorerPanel = defineAsyncComponent(
  () => import('@/components/analytics/UserExplorerPanel.vue'),
);
const ExploreTool = defineAsyncComponent(() => import('@/components/explore/ExploreTool.vue'));
const InsightsView = defineAsyncComponent(() => import('@/components/insights/InsightsView.vue'));
const TextEnrichmentView = defineAsyncComponent(
  () => import('@/components/textenrichment/TextEnrichmentView.vue'),
);
const TextExploreTool = defineAsyncComponent(
  () => import('@/components/textexplore/TextExploreTool.vue'),
);
const WebSourceSettings = defineAsyncComponent(
  () => import('@/components/settings/WebSourceSettings.vue'),
);
const ConnectorSourceSettings = defineAsyncComponent(
  () => import('@/components/settings/ConnectorSourceSettings.vue'),
);

const props = defineProps<{
  sourceId: string;
  tool: string;
  table?: string;
}>();

const { sourceById } = useSources();
const sourceStore = useSourceStore();

const source = computed(() => sourceById(props.sourceId));
const activeTool = computed(() => (props.tool || 'dashboard') as ToolId);

const toolLabel = computed(() => {
  const def = TOOL_DEFS[activeTool.value];
  return def?.label ?? activeTool.value;
});

const showPeriod = computed(
  () =>
    source.value?.kind === 'web-analytics' &&
    ['dashboard', 'funnels', 'retention'].includes(activeTool.value),
);

// Extract the raw event source ID (strip "web:" prefix) for analytics API calls
const eventSourceId = computed(() => {
  const id = source.value?.id ?? '';
  return id.startsWith('web:') ? id.slice(4) : id;
});

// Fetch event names for funnel/retention when on a web-analytics source
const { data: eventList } = useQuery({
  key: () => ['pa-event-names', eventSourceId.value, sourceStore.period],
  query: async () => await productAnalyticsApi.events(eventSourceId.value, sourceStore.period),
  enabled: () =>
    source.value?.kind === 'web-analytics' &&
    ['funnels', 'retention', 'events'].includes(activeTool.value),
});

const eventNames = computed(() => (eventList.value ?? []).map((e) => e.name));
</script>

<template>
  <UDashboardPanel id="source">
    <template #header>
      <UDashboardNavbar :title="toolLabel">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
      </UDashboardNavbar>
      <UDashboardToolbar v-if="showPeriod">
        <template #left>
          <PeriodSelector v-model="sourceStore.period" />
        </template>
      </UDashboardToolbar>
    </template>

    <template #body>
      <!-- Dashboard -->
      <template v-if="activeTool === 'dashboard'">
        <WebDashboard
          v-if="source?.kind === 'web-analytics'"
          :source-id="eventSourceId"
          :period="sourceStore.period"
        />
        <ConnectorDashboard v-else-if="source" :source="source" :source-id="sourceId" />
      </template>

      <!-- Funnels -->
      <div v-else-if="activeTool === 'funnels'" class="p-4">
        <div class="rounded-lg border border-default bg-elevated p-4">
          <FunnelPanel
            :source-id="eventSourceId"
            :period="sourceStore.period"
            :event-names="eventNames"
          />
        </div>
      </div>

      <!-- Retention -->
      <div v-else-if="activeTool === 'retention'" class="p-4">
        <div class="rounded-lg border border-default bg-elevated p-4">
          <RetentionPanel
            :source-id="eventSourceId"
            :period="sourceStore.period"
            :event-names="eventNames"
          />
        </div>
      </div>

      <!-- Users -->
      <div v-else-if="activeTool === 'users'" class="p-4">
        <div class="rounded-lg border border-default bg-elevated p-4">
          <UserExplorerPanel :source-id="eventSourceId" />
        </div>
      </div>

      <!-- Explore -->
      <ExploreTool v-else-if="activeTool === 'explore'" :source-id="sourceId" :table="table" />

      <!-- Insights -->
      <InsightsView v-else-if="activeTool === 'insights'" :source-id="sourceId" :table="table" />

      <!-- Text enrichment -->
      <TextEnrichmentView
        v-else-if="activeTool === 'textenrichment'"
        :source-id="sourceId"
        :table="table"
      />

      <!-- Text Explorer -->
      <TextExploreTool
        v-else-if="activeTool === 'textexplore'"
        :source-id="sourceId"
        :table="table"
      />

      <!-- Settings -->
      <template v-else-if="activeTool === 'settings'">
        <WebSourceSettings v-if="source?.kind === 'web-analytics'" :source="source" />
        <ConnectorSourceSettings v-else-if="source" :source="source" />
      </template>
    </template>
  </UDashboardPanel>
</template>

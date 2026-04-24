<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { ref, computed } from 'vue';

import EventListPanel from '@/components/analytics/EventListPanel.vue';
import FunnelPanel from '@/components/analytics/FunnelPanel.vue';
import RetentionPanel from '@/components/analytics/RetentionPanel.vue';
import UserExplorerPanel from '@/components/analytics/UserExplorerPanel.vue';
import PeriodSelector from '@/components/layout/PeriodSelector.vue';
import { productAnalyticsApi } from '@/services/api';

const props = defineProps<{
  sourceId: string;
}>();

const tab = ref<'events' | 'funnels' | 'retention' | 'users'>('events');
const period = ref('30d');

const tabs = [
  { label: 'Events', value: 'events' as const },
  { label: 'Funnels', value: 'funnels' as const },
  { label: 'Retention', value: 'retention' as const },
  { label: 'Users', value: 'users' as const },
];

// Fetch event names for funnel/retention dropdowns
const { data: eventList } = useQuery({
  key: () => ['pa-event-names', props.sourceId, period.value],
  query: async () => await productAnalyticsApi.events(props.sourceId, period.value),
});

const eventNames = computed(() => (eventList.value ?? []).map((e) => e.name));
</script>

<template>
  <div>
    <!-- Sub-tabs and period selector -->
    <div class="mb-6 flex items-center justify-between">
      <div class="flex items-center gap-1 rounded-lg bg-elevated p-0.5">
        <button
          v-for="t in tabs"
          :key="t.value"
          class="rounded-md px-3 py-1 text-sm font-medium transition-colors"
          :class="
            tab === t.value
              ? 'bg-default text-highlighted shadow-sm'
              : 'cursor-pointer text-muted hover:text-highlighted'
          "
          @click="tab = t.value"
        >
          {{ t.label }}
        </button>
      </div>

      <PeriodSelector
        v-if="tab !== 'users'"
        v-model="period"
        :periods="[
          { label: '7 days', value: '7d' },
          { label: '30 days', value: '30d' },
          { label: '90 days', value: '12m' },
        ]"
      />
    </div>

    <!-- Tab content -->
    <div class="rounded-lg border border-default bg-elevated p-4">
      <h3 class="mb-4 text-sm font-medium text-highlighted">
        {{ tabs.find((t) => t.value === tab)?.label }}
      </h3>
      <EventListPanel v-if="tab === 'events'" :source-id="sourceId" :period="period" />
      <FunnelPanel
        v-else-if="tab === 'funnels'"
        :source-id="sourceId"
        :period="period"
        :event-names="eventNames"
      />
      <RetentionPanel
        v-else-if="tab === 'retention'"
        :source-id="sourceId"
        :period="period"
        :event-names="eventNames"
      />
      <UserExplorerPanel v-else-if="tab === 'users'" :source-id="sourceId" />
    </div>
  </div>
</template>

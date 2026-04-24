<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { computed } from 'vue';

import { productAnalyticsApi } from '@/services/api';
import type { EventListRow } from '@/types';

const props = defineProps<{
  sourceId: string;
  period: string;
}>();

const { data: events } = useQuery({
  key: () => ['pa-events', props.sourceId, props.period],
  query: async () => await productAnalyticsApi.events(props.sourceId, props.period),
});

const eventList = computed<EventListRow[]>(() => events.value ?? []);
</script>

<template>
  <div>
    <table v-if="eventList.length > 0" class="w-full text-sm">
      <thead>
        <tr class="border-b border-default text-sm text-muted">
          <th class="pb-2 text-left font-medium">Event</th>
          <th class="pb-2 text-right font-medium">Count</th>
          <th class="pb-2 text-right font-medium">Unique Users</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in eventList" :key="row.name" class="border-b border-default last:border-0">
          <td class="py-2 text-highlighted">{{ row.name }}</td>
          <td class="py-2 text-right text-muted">{{ Number(row.count).toLocaleString() }}</td>
          <td class="py-2 text-right text-muted">
            {{ Number(row.uniqueUsers).toLocaleString() }}
          </td>
        </tr>
      </tbody>
    </table>
    <p v-else class="py-8 text-center text-sm text-muted">No events recorded yet</p>
  </div>
</template>

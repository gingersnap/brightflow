<script setup lang="ts">
/**
 * Events tab of Product Analytics: a read-only table of per-event totals and
 * unique-user counts for the selected source and period. No local state —
 * one colada query keyed on the props, straight into UTable.
 */

import type { TableColumn } from '@nuxt/ui';
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

const columns: TableColumn<EventListRow>[] = [
  { accessorKey: 'name', header: 'Event' },
  {
    accessorKey: 'count',
    header: 'Count',
    cell: ({ row }) => Number(row.original.count).toLocaleString(),
    meta: { class: { th: 'text-right', td: 'text-right' } },
  },
  {
    accessorKey: 'uniqueUsers',
    header: 'Unique Users',
    cell: ({ row }) => Number(row.original.uniqueUsers).toLocaleString(),
    meta: { class: { th: 'text-right', td: 'text-right' } },
  },
];
</script>

<template>
  <UTable :data="eventList" :columns="columns" empty="No events recorded yet" />
</template>

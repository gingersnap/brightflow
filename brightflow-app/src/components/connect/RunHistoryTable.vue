<script setup lang="ts">
import type { TableColumn } from '@nuxt/ui';
import { h } from 'vue';

import type { EnrichedSyncRun } from '@/types';

import SyncStatusBadge from './SyncStatusBadge.vue';

defineProps<{
  runs: EnrichedSyncRun[];
}>();

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  if (diff < 60_000) {
    return 'just now';
  }
  if (diff < 3_600_000) {
    return `${Math.round(diff / 60_000)}m ago`;
  }
  if (diff < 86_400_000) {
    return `${Math.round(diff / 3_600_000)}h ago`;
  }
  return `${Math.round(diff / 86_400_000)}d ago`;
}

function duration(startedAt: string, finishedAt: string): string {
  const ms = new Date(finishedAt).getTime() - new Date(startedAt).getTime();
  return `${Math.round(ms / 1000)}s`;
}

const columns: TableColumn<EnrichedSyncRun>[] = [
  {
    accessorKey: 'status',
    header: 'Status',
    cell: ({ row }) =>
      h(SyncStatusBadge, {
        status: row.original.status as 'pending' | 'running' | 'completed' | 'failed',
      }),
  },
  { accessorKey: 'connectorName', header: 'Connector' },
  {
    accessorKey: 'startedAt',
    header: 'Started',
    cell: ({ row }) => relativeTime(row.original.startedAt),
  },
  {
    accessorKey: 'finishedAt',
    header: 'Duration',
    cell: ({ row }) =>
      row.original.finishedAt ? duration(row.original.startedAt, row.original.finishedAt) : '—',
  },
  {
    accessorKey: 'rowsSynced',
    header: 'Rows',
    cell: ({ row }) =>
      row.original.rowsSynced > 0 ? row.original.rowsSynced.toLocaleString() : '—',
  },
  {
    accessorKey: 'error',
    header: 'Error',
    cell: ({ row }) =>
      row.original.status === 'failed' && row.original.error ? row.original.error : '—',
    meta: { class: { td: 'break-all' } },
  },
];
</script>

<template>
  <UTable :data="runs" :columns="columns" empty="No runs yet" />
</template>

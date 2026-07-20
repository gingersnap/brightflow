<script setup lang="ts">
import type { TableColumn, TableRow, ContextMenuItem } from '@nuxt/ui';
import { computed, h, ref } from 'vue';

import type { EnrichedSyncRun } from '@/types';

import SyncStatusBadge from './SyncStatusBadge.vue';

const props = defineProps<{
  runs: EnrichedSyncRun[];
}>();

const emit = defineEmits<{
  run: [connectorName: string];
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

// ── Row context menu (single shared instance) ──────────────────────────────
// Documented UTable pattern: one UContextMenu wraps the table, controlled via
// V-model:open; @contextmenu on UTable gives us the right-clicked row.
const open = ref(false);
const target = ref<EnrichedSyncRun | null>(null);
const hovered = ref<EnrichedSyncRun | null>(null);

function onContextMenu(e: Event, row: TableRow<EnrichedSyncRun>): void {
  target.value = row.original;
  e.preventDefault();
  open.value = true;
}

function onHover(_e: Event, row: TableRow<EnrichedSyncRun> | null): void {
  hovered.value = row?.original ?? null;
}

// `target` is set on right-click (menu); `hovered` drives the keyboard
// Shortcut so ⌘↵ re-runs the row under the cursor without opening the menu.
const activeRow = computed(() => target.value ?? hovered.value);

const items = computed<ContextMenuItem[][]>(() => {
  const row = activeRow.value;
  if (row == null) {
    return [];
  }
  return [
    [
      {
        label: 'Re-run sync',
        icon: 'i-lucide-refresh-cw',
        kbds: ['meta', 'enter'],
        onSelect: () => emit('run', row.connectorName),
      },
      {
        label: 'Copy error',
        icon: 'i-lucide-copy',
        disabled: !row.error,
        onSelect: () => {
          if (row.error) {
            void navigator.clipboard.writeText(row.error);
          }
        },
      },
    ],
  ];
});

// ⌘↵ only fires while a row is hovered, so multiple RunHistoryTables on the
// Same page don't all re-run their last target.
defineShortcuts(computed(() => (hovered.value ? extractShortcuts(items.value) : {})));
</script>

<template>
  <UContextMenu v-model:open="open" :items="items">
    <UTable
      :data="props.runs"
      :columns="columns"
      empty="No runs yet"
      @contextmenu="onContextMenu"
      @hover="onHover"
    />
  </UContextMenu>
</template>

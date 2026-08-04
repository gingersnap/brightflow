<script setup lang="ts">
/**
 * Table of sync runs (status, timing, rows, error) with a shared row context
 * menu offering re-run and copy-error. The ⌘↵ re-run shortcut is deliberately
 * hover-scoped: it acts on the row under the cursor and stays inert
 * otherwise, so several of these tables can coexist on one page.
 */

import type { TableColumn, TableRow, ContextMenuItem } from '@nuxt/ui';
import { formatTimeAgo, useClipboard, useNow } from '@vueuse/core';
import { computed, h, ref } from 'vue';

const { copy } = useClipboard();

import type { EnrichedSyncRun } from '@/types';

import SyncStatusBadge from './SyncStatusBadge.vue';

const props = defineProps<{
  runs: EnrichedSyncRun[];
}>();

const emit = defineEmits<{
  run: [connectorName: string];
}>();

const now = useNow({ interval: 30_000 });

/** Rows are render functions, so per-row useTimeAgo can't be used; reading
 * `now` here makes the cells re-derive as time passes. */
function relativeTime(dateStr: string): string {
  return formatTimeAgo(new Date(dateStr), {}, now.value);
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
            void copy(row.error);
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

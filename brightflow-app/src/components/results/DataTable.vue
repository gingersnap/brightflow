<script setup lang="ts">
/**
 * Plain table view of query results. Adapts the store's columnar rows to
 * UTable's object shape, formatting at transform time (locale numbers,
 * strings truncated at 100 chars). The context menu offers clipboard copy of
 * a column name or cell value; richer actions are deferred (see below).
 */

import type { ContextMenuItem, TableColumn } from '@nuxt/ui';
import { useClipboard } from '@vueuse/core';
import { computed, h, ref } from 'vue';

import { useResultsStore } from '@/stores/results';
import { formatNumber } from '@/utils/format';

const resultsStore = useResultsStore();

type RowData = Record<string, unknown>;

/*
 * Build table columns for UTable (TanStack Table format). `id` + `accessorFn`
 * rather than `accessorKey`: TanStack treats dots in accessorKey as deep
 * paths, so a column literally named "a.b" would resolve to row.a.b and come
 * back empty. Header and cell render functions carry the context-menu wiring,
 * so the menu target comes from the render tree instead of DOM sniffing.
 */
const tableColumns = computed<TableColumn<RowData>[]>(() =>
  resultsStore.table.columns.map((col) => ({
    accessorFn: (row: RowData) => row[col.name],
    cell: ({ getValue }) => {
      const value = String(getValue() ?? '');
      return h(
        'span',
        {
          onContextmenu: (e: MouseEvent) => {
            openMenu(e, { colName: col.name, value });
          },
        },
        value,
      );
    },
    header: () =>
      h(
        'span',
        {
          onContextmenu: (e: MouseEvent) => {
            openMenu(e, { colName: col.name });
          },
        },
        col.name,
      ),
    id: col.name,
  })),
);

// Transform rows array to objects for UTable
const tableData = computed(() =>
  resultsStore.table.rows.map((row, index) => {
    const obj: Record<string, unknown> = { _index: index };
    resultsStore.table.columns.forEach((col, i) => {
      obj[col.name] = formatCell(row[i], col.dtype);
    });
    return obj;
  }),
);

function formatCell(value: unknown, dtype: string): string {
  if (value === null || value === undefined) {
    return '—';
  }

  switch (dtype) {
    case 'int':
    case 'float': {
      return formatNumber(Number(value));
    }
    case 'string': {
      // Truncate long strings
      const str = String(value);
      return str.length > 100 ? `${str.slice(0, 100)}...` : str;
    }
    default: {
      return String(value);
    }
  }
}

/*
 * Context menu (clipboard-only v1). Sort / group-by / filter-by-this-value
 * are deferred — wiring them needs query-store manipulation (see plan OQ2).
 */
const open = ref(false);
const isHeader = ref(false);
const colName = ref<string | null>(null);
const cellValue = ref<string | null>(null);

const { copy } = useClipboard();

function openMenu(e: MouseEvent, target: { colName: string; value?: string }): void {
  e.preventDefault();
  isHeader.value = target.value === undefined;
  colName.value = target.colName;
  cellValue.value = target.value ?? null;
  open.value = true;
}

const items = computed<ContextMenuItem[][]>(() => {
  if (isHeader.value) {
    const name = colName.value;
    if (name == null) {
      return [];
    }
    return [[{ label: 'Copy column name', icon: 'i-lucide-copy', onSelect: () => copy(name) }]];
  }
  const value = cellValue.value;
  if (value == null) {
    return [];
  }
  return [[{ label: 'Copy cell value', icon: 'i-lucide-copy', onSelect: () => copy(value) }]];
});
</script>

<template>
  <UContextMenu v-model:open="open" :items="items">
    <!-- The no-op prevent covers right-clicks on td padding (outside the
         rendered span): no app menu there, but no browser menu either. -->
    <div class="h-full overflow-auto" @contextmenu.prevent>
      <UTable :data="tableData" :columns="tableColumns" class="w-full" />
    </div>
  </UContextMenu>
</template>

<style scoped></style>

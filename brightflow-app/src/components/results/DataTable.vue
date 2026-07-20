<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui';
import { computed, ref } from 'vue';

import { useResultsStore } from '@/stores/results';

const resultsStore = useResultsStore();

// Build table columns for UTable (TanStack Table format)
const tableColumns = computed(() =>
  resultsStore.columns.map((col) => ({
    accessorKey: col.name,
    header: col.name,
  })),
);

// Transform rows array to objects for UTable
const tableData = computed(() =>
  resultsStore.rows.map((row, index) => {
    const obj: Record<string, unknown> = { _index: index };
    resultsStore.columns.forEach((col, i) => {
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
    case 'int': {
      return Number(value).toLocaleString();
    }
    case 'float': {
      return Number(value).toLocaleString(undefined, {
        minimumFractionDigits: 0,
        maximumFractionDigits: 2,
      });
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

// ── Context menu (clipboard-only v1) ───────────────────────────────────────
// One UContextMenu wraps the table; a native @contextmenu on the wrapper
// Inspects whether the cursor is on a header (<th>) or body (<td>) cell and
// Captures the column / value. Sort / group-by / filter-by-this-value are
// Deferred — wiring them needs query-store manipulation (see plan OQ2).
const open = ref(false);
const isHeader = ref(false);
const colName = ref<string | null>(null);
const cellValue = ref<string | null>(null);

function copy(text: string): void {
  void navigator.clipboard.writeText(text);
}

function onContextMenu(e: MouseEvent): void {
  const target = e.target as HTMLElement | null;
  const th = target?.closest('th') ?? null;
  const td = target?.closest('td') ?? null;

  if (th != null) {
    const col = tableColumns.value[th.cellIndex];
    if (col == null) {
      return;
    }
    isHeader.value = true;
    colName.value = col.header;
    cellValue.value = null;
  } else if (td == null) {
    return;
  } else {
    const col = tableColumns.value[td.cellIndex];
    const tr = td.closest('tr');
    if (col == null || tr == null) {
      return;
    }
    // RowIndex counts the header row; offset by one for the body index.
    const row = tableData.value[tr.rowIndex - 1];
    if (row == null) {
      return;
    }
    const value = row[col.accessorKey];
    isHeader.value = false;
    colName.value = col.accessorKey;
    cellValue.value = typeof value === 'string' ? value : String(value ?? '');
  }
  e.preventDefault();
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
    <div class="h-full overflow-auto" @contextmenu="onContextMenu">
      <UTable :data="tableData" :columns="tableColumns" class="w-full" />
    </div>
  </UContextMenu>
</template>

<style scoped></style>

<script setup lang="ts">
import { computed } from 'vue';

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
</script>

<template>
  <div class="overflow-auto h-full">
    <UTable :data="tableData" :columns="tableColumns" class="w-full" />
  </div>
</template>

<style scoped>
/* Override table styles for data display */
:deep(table) {
  font-size: 0.875rem;
}

:deep(th) {
  position: sticky;
  top: 0;
  z-index: 1;
}

:deep(td) {
  white-space: nowrap;
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>

<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { Table2 } from 'lucide-vue-next';
import { computed } from 'vue';

import { tableApi } from '@/services/api';
import { useSourceStore } from '@/stores/source';
import type { SourceTable } from '@/types';

const sourceStore = useSourceStore();

defineEmits<{
  'select-table': [table: SourceTable];
}>();

// Fallback: fetch full table list if source has no tables
const { data: allTables } = useQuery({
  key: ['available-tables'],
  query: async () => {
    const result = await tableApi.listAvailable();
    return result ?? [];
  },
  enabled: () => (sourceStore.selectedSource?.tables.length ?? 0) === 0,
});

const sourceTables = computed(() => {
  const src = sourceStore.selectedSource;
  if (src && src.tables.length > 0) {
    return src.tables;
  }
  return (allTables.value ?? []).map((t) => ({
    name: t.name,
    numRows: t.numRows ?? null,
  }));
});
</script>

<template>
  <div class="p-6">
    <h2 class="mb-4 text-lg font-semibold text-highlighted">Select a table to explore</h2>
    <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
      <button
        v-for="table in sourceTables"
        :key="table.name"
        class="flex cursor-pointer items-center gap-3 rounded-lg border border-default bg-elevated p-4 text-left transition-all hover:border-primary-500/50 hover:shadow-sm"
        @click="$emit('select-table', table)"
      >
        <Table2 class="h-5 w-5 text-muted" />
        <div>
          <p class="text-sm font-medium text-highlighted">{{ table.name }}</p>
          <p v-if="table.numRows != null" class="text-xs text-muted">
            {{ table.numRows.toLocaleString() }} rows
          </p>
        </div>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Table2 } from 'lucide-vue-next';
import { computed } from 'vue';

import { useSourceStore } from '@/stores/source';
import type { SourceTable } from '@/types';

const props = defineProps<{
  sourceId: string;
}>();

defineEmits<{
  'select-table': [table: SourceTable];
}>();

const sourceStore = useSourceStore();

const sourceTables = computed(() => {
  const src = sourceStore.getSourceById(props.sourceId);
  return src?.tables ?? [];
});
</script>

<template>
  <div class="p-4">
    <h2 class="mb-3 text-sm font-semibold text-highlighted">Select a table to explore</h2>
    <div
      v-if="sourceTables.length > 0"
      class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3"
    >
      <button
        v-for="table in sourceTables"
        :key="table.name"
        class="flex cursor-pointer items-center gap-3 rounded-lg border border-default bg-elevated p-4 text-left transition-all hover:border-primary-500/50 hover:shadow-sm"
        @click="$emit('select-table', table)"
      >
        <Table2 class="h-5 w-5 text-muted" />
        <div>
          <p class="text-sm font-medium text-highlighted">{{ table.name }}</p>
          <p v-if="table.numRows != null" class="text-sm text-muted">
            {{ table.numRows.toLocaleString() }} rows
          </p>
        </div>
      </button>
    </div>
    <p v-else class="text-muted">No tables available for this source.</p>
  </div>
</template>

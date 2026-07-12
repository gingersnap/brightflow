<script setup lang="ts">
import { ChevronDown, ChevronRight, Table2 } from 'lucide-vue-next';
import { computed, ref, watch } from 'vue';

import { useSourceStore } from '@/stores/source';
import type { SourceTable } from '@/types';

const props = defineProps<{
  sourceId: string;
  selectedTable?: string;
}>();

const emit = defineEmits<{
  'select-table': [table: SourceTable];
  'auto-select-table': [table: SourceTable];
}>();

const sourceStore = useSourceStore();

const sourceTables = computed(() => {
  const src = sourceStore.getSourceById(props.sourceId);
  return src?.tables ?? [];
});

const selected = computed(() => sourceTables.value.find((t) => t.name === props.selectedTable));

const collapsed = ref(props.selectedTable != null);

watch(
  () => props.selectedTable,
  (name) => {
    collapsed.value = name != null;
  },
);

// Auto-select the only table on single-table sources
watch(
  () => [sourceTables.value, props.selectedTable] as const,
  ([tables, selectedName]) => {
    const only = tables[0];
    if (selectedName == null && tables.length === 1 && only) {
      emit('auto-select-table', only);
    }
  },
  { immediate: true },
);

function handleCardClick(table: SourceTable): void {
  collapsed.value = true;
  if (table.name !== props.selectedTable) {
    emit('select-table', table);
  }
}
</script>

<template>
  <div class="border-b border-default">
    <!-- Section Header -->
    <button
      class="flex w-full items-center gap-2 bg-muted/30 px-4 py-2 text-left transition-colors hover:bg-muted/40"
      @click="collapsed = !collapsed"
    >
      <component :is="collapsed ? ChevronRight : ChevronDown" class="h-4 w-4 text-muted" />
      <h2 class="text-sm font-medium text-default">
        {{ selectedTable ? `Table: ${selectedTable}` : 'Table' }}
      </h2>
      <span v-if="selected?.numRows != null" class="text-xs text-muted">
        ({{ selected.numRows.toLocaleString() }} rows)
      </span>
      <span v-else-if="!selectedTable" class="text-xs text-muted">(none selected)</span>
    </button>

    <!-- Content -->
    <div v-if="!collapsed" class="border-t border-default bg-muted/10 p-4">
      <div
        v-if="sourceTables.length > 0"
        class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3"
      >
        <button
          v-for="table in sourceTables"
          :key="table.name"
          class="flex cursor-pointer items-center gap-3 rounded-lg border bg-elevated p-4 text-left transition-all hover:shadow-sm"
          :class="
            table.name === selectedTable
              ? 'border-primary-500 ring-1 ring-primary-500/40'
              : 'border-default hover:border-primary-500/50'
          "
          :aria-pressed="table.name === selectedTable"
          @click="handleCardClick(table)"
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
  </div>
</template>

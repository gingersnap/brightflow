<script setup lang="ts">
/**
 * Collapsible table picker for table-scoped tools. Expands itself whenever
 * no table is selected and collapses on pick. Single-table sources emit
 * `auto-select-table` instead of `select-table`, so the parent can give
 * automatic selection different navigation semantics than a user click.
 */

import { computed, ref, watch } from 'vue';

import CollapsibleSection from '@/components/common/CollapsibleSection.vue';
import { useSources } from '@/composables/useSources';
import type { SourceTable } from '@/types';
import type { DeclarationDiff } from '@/types/generated';

const props = defineProps<{
  sourceId: string;
  selectedTable?: string | undefined;
  /** Only offer tables that support text enrichment (Topics). */
  enrichableOnly?: boolean;
}>();

const emit = defineEmits<{
  'select-table': [table: SourceTable];
  'auto-select-table': [table: SourceTable];
}>();

const { sourceById } = useSources();

const sourceTables = computed(() => {
  const src = sourceById(props.sourceId);
  const tables = src?.tables ?? [];
  return props.enrichableOnly ? tables.filter((t) => t.enrichable) : tables;
});

const selected = computed(() => sourceTables.value.find((t) => t.name === props.selectedTable));

const expanded = ref(props.selectedTable == null);

watch(
  () => props.selectedTable,
  (name) => {
    expanded.value = name == null;
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

/** "Declaration 0.2.0 → 0.3.0 changed reactions_total, created_at". */
function changeSummary(diff: DeclarationDiff): string {
  const columns: string[] = [];
  for (const change of diff.changes) {
    if (change.column != null && !columns.includes(change.column)) {
      columns.push(change.column);
    }
  }
  const versions =
    diff.fromVersion != null && diff.toVersion != null
      ? `${diff.fromVersion} → ${diff.toVersion}`
      : (diff.toVersion ?? '');
  const what = columns.length > 0 ? columns.join(', ') : 'table settings';
  return `Declaration ${versions} changed ${what}`.replace('  ', ' ');
}

/** One line per changed field, for the tooltip. */
function changeDetail(diff: DeclarationDiff): string {
  return diff.changes
    .map((c) => `${c.column ?? 'table'}.${c.field}: ${c.from ?? '—'} → ${c.to ?? '—'}`)
    .join('\n');
}

function handleCardClick(table: SourceTable): void {
  expanded.value = false;
  if (table.name !== props.selectedTable) {
    emit('select-table', table);
  }
}
</script>

<template>
  <CollapsibleSection v-model:open="expanded" class="border-b border-default">
    <template #title>
      <h2 class="text-sm font-medium text-default">
        {{ selectedTable ? `Table: ${selectedTable}` : 'Table' }}
      </h2>
      <span v-if="selected?.numRows != null" class="text-xs text-muted">
        ({{ selected.numRows.toLocaleString() }} rows)
      </span>
      <span v-else-if="!selectedTable" class="text-xs text-muted">(none selected)</span>
    </template>

    <!-- Content -->
    <div class="border-t border-default bg-muted/10 p-4">
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
          <UIcon name="i-lucide-table-2" class="h-5 w-5 text-muted" />
          <div class="min-w-0">
            <p class="text-sm font-medium text-highlighted">
              {{ table.displayName ?? table.name }}
              <span v-if="table.displayName" class="font-normal text-muted">{{ table.name }}</span>
            </p>
            <p
              v-if="table.description"
              class="truncate text-sm text-muted"
              :title="table.description"
            >
              {{ table.description }}
            </p>
            <p v-if="table.numRows != null" class="text-sm text-muted">
              {{ table.numRows.toLocaleString() }} rows
            </p>
            <p
              v-if="table.lastDeclarationChange"
              class="truncate text-sm text-muted"
              :title="changeDetail(table.lastDeclarationChange)"
            >
              <UIcon name="i-lucide-git-commit-horizontal" class="mr-1 inline h-3.5 w-3.5" />
              {{ changeSummary(table.lastDeclarationChange) }}
            </p>
          </div>
        </button>
      </div>
      <p v-else class="text-muted">No tables available for this source.</p>
    </div>
  </CollapsibleSection>
</template>

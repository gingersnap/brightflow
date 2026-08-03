<script setup lang="ts">
/**
 * Route-level Enrich view: table picker on top, then a master–detail split
 * of ColumnListPanel and FunctionEditor. Owns the function-list and schema
 * queries plus the selection state (selected function vs pending create
 * kind), auto-selecting the first function when a table's list loads.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import { enrichFnApi, tableApi } from '@/services/api';
import type { SourceTable } from '@/types';
import type { FunctionKind } from '@/types/enrichment';

import ColumnListPanel from './ColumnListPanel.vue';
import FunctionEditor from './FunctionEditor.vue';

const props = defineProps<{
  sourceId: string;
  table?: string;
}>();

const router = useRouter();
const queryCache = useQueryCache();

const activeTable = computed(() => props.table);

function handleSelectTable(table: SourceTable): void {
  router.push({ name: 'enrich-table', params: { sourceId: props.sourceId, table: table.name } });
}

function handleAutoSelectTable(table: SourceTable): void {
  router.replace({ name: 'enrich-table', params: { sourceId: props.sourceId, table: table.name } });
}

// Functions on this table
const functionsKey = computed(() => ['enrich-fns', props.sourceId, activeTable.value]);
const { data: functions } = useQuery({
  key: () => functionsKey.value,
  query: async () => (await enrichFnApi.list(props.sourceId, activeTable.value ?? '')) ?? [],
  enabled: () => activeTable.value != null,
});

// Source columns from the table index schema
const { data: tableIndex } = useQuery({
  key: () => ['tables-index', props.sourceId],
  query: async () => (await tableApi.listAvailable()) ?? [],
  enabled: () => activeTable.value != null,
});

const sourceColumns = computed(() => {
  const info = (tableIndex.value ?? []).find(
    (t) => t.sourceId === props.sourceId && t.name === activeTable.value,
  );
  const schema = info?.schema as { fields?: { name?: string; type?: string }[] } | null;
  return (schema?.fields ?? [])
    .filter((f) => typeof f.name === 'string')
    .map((f) => ({ dtype: f.type ?? '', name: f.name ?? '' }));
});

// Master–detail selection
const selectedFnId = ref<string | null>(null);
const createKind = ref<FunctionKind | null>(null);

const selectedFn = computed(
  () => (functions.value ?? []).find((f) => f.id === selectedFnId.value) ?? null,
);

// Auto-select the first function when the table's list loads
watch(
  () => [activeTable.value, functions.value] as const,
  ([, list]) => {
    if (selectedFnId.value == null && createKind.value == null && list != null) {
      selectedFnId.value = list[0]?.id ?? null;
    }
  },
  { immediate: true },
);

watch(activeTable, () => {
  selectedFnId.value = null;
  createKind.value = null;
});

function handleSelect(id: string): void {
  createKind.value = null;
  selectedFnId.value = id;
}

function handleCreate(kind: FunctionKind): void {
  selectedFnId.value = null;
  createKind.value = kind;
}

async function handleSaved(id: string): Promise<void> {
  createKind.value = null;
  selectedFnId.value = id;
  await queryCache.invalidateQueries({ key: functionsKey.value });
}

async function handleDeleted(): Promise<void> {
  selectedFnId.value = null;
  await queryCache.invalidateQueries({ key: functionsKey.value });
}
</script>

<template>
  <div class="flex h-full flex-col">
    <TableSectionPane
      :source-id="sourceId"
      :selected-table="table"
      @select-table="handleSelectTable"
      @auto-select-table="handleAutoSelectTable"
    />

    <div v-if="activeTable == null" class="p-6">
      <p class="text-sm text-muted">Choose a table above to manage its derived columns.</p>
    </div>

    <div v-else class="grid min-h-0 flex-1 grid-cols-[320px_1fr]">
      <ColumnListPanel
        :source-columns="sourceColumns"
        :functions="functions ?? []"
        :selected-id="selectedFnId"
        @select="handleSelect"
        @create="handleCreate"
      />
      <div class="min-h-0 overflow-y-auto">
        <FunctionEditor
          v-if="selectedFn != null || createKind != null"
          :source-id="sourceId"
          :table="activeTable"
          :columns="sourceColumns"
          :fn="selectedFn"
          :create-kind="createKind"
          @saved="handleSaved"
          @deleted="handleDeleted"
        />
        <div v-else class="p-6">
          <p class="text-sm text-muted">
            Select a derived column on the left, or add one to enrich this table with an AI prompt.
          </p>
        </div>
      </div>
    </div>
  </div>
</template>

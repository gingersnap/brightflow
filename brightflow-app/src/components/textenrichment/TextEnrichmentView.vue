<script setup lang="ts">
/**
 * Route-level Text enrichment view: table picker on top, then one tab per
 * text function. Classification holds the classify function, the
 * category tree it reads and what it wrote; Extraction holds the extract
 * function, the vocabularies it reads, the mentions it wrote and the
 * unresolved-subject queue. The tab split follows the data: neither call
 * reads the other's vocabularies. The action log is on the Activity page
 * under Settings, not in this tool.
 */

import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';

import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import type { SourceTable } from '@/types';

import ClassificationPane from './ClassificationPane.vue';
import ExtractionPane from './ExtractionPane.vue';

const props = defineProps<{
  sourceId: string;
  table?: string | undefined;
}>();

const router = useRouter();
const activeTable = computed(() => props.table);

type Pane = 'classification' | 'extraction';
const pane = ref<Pane>('classification');
const panes: { id: Pane; label: string; icon: string }[] = [
  { id: 'classification', label: 'Classification', icon: 'i-lucide-tags' },
  { id: 'extraction', label: 'Extraction', icon: 'i-lucide-quote' },
];

function handleSelectTable(table: SourceTable): void {
  router.push({
    name: 'textenrichment-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

function handleAutoSelectTable(table: SourceTable): void {
  router.replace({
    name: 'textenrichment-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
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
      <p class="text-sm text-muted">Choose a table above to set up text enrichment.</p>
    </div>

    <template v-else>
      <div class="flex items-center gap-1 border-b border-default px-4 py-2">
        <UButton
          v-for="p in panes"
          :key="p.id"
          size="md"
          :color="pane === p.id ? 'primary' : 'neutral'"
          :variant="pane === p.id ? 'soft' : 'ghost'"
          :icon="p.icon"
          @click="pane = p.id"
        >
          {{ p.label }}
        </UButton>
      </div>

      <div class="flex-1 overflow-auto p-4">
        <ClassificationPane
          v-if="pane === 'classification'"
          :source-id="sourceId"
          :table="activeTable"
        />
        <ExtractionPane v-else :source-id="sourceId" :table="activeTable" />
      </div>
    </template>
  </div>
</template>

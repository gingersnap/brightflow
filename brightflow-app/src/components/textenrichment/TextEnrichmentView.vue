<script setup lang="ts">
/**
 * Route-level Text enrichment view: table picker on top, then two panes.
 * Setup is the vocabularies and the two text functions (classify, extract);
 * Results is what they materialised — row grain, mention grain, the
 * unresolved-subject queue and vocabulary health. The action log is on the
 * Activity page under Settings, not in this tool.
 */

import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';

import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import type { SourceTable } from '@/types';

import ResultsPane from './ResultsPane.vue';
import SetupPane from './SetupPane.vue';

const props = defineProps<{
  sourceId: string;
  table?: string | undefined;
}>();

const router = useRouter();
const activeTable = computed(() => props.table);

type Pane = 'setup' | 'results';
const pane = ref<Pane>('setup');
const panes: { id: Pane; label: string; icon: string }[] = [
  { id: 'setup', label: 'Setup', icon: 'i-lucide-sliders-horizontal' },
  { id: 'results', label: 'Results', icon: 'i-lucide-chart-bar' },
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
        <SetupPane v-if="pane === 'setup'" :source-id="sourceId" :table="activeTable" />
        <ResultsPane v-else :source-id="sourceId" :table="activeTable" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, toRef } from 'vue';
import { useRouter } from 'vue-router';

import TableSectionPane from '@/components/sources/TableSectionPane.vue';
import { useSources } from '@/composables/useSources';
import { useTextExplore } from '@/composables/useTextExplore';
import type { SourceTable } from '@/types';

import ResultsList from './ResultsList.vue';
import TermInputBar from './TermInputBar.vue';
import WordsPanel from './WordsPanel.vue';

const props = defineProps<{
  sourceId: string;
  table?: string;
}>();

const router = useRouter();
const { sourceById } = useSources();

const enrichableTables = computed(
  () => sourceById(props.sourceId)?.tables.filter((t) => t.enrichable) ?? [],
);
const activeTable = computed(() => props.table);

function handleSelectTable(table: SourceTable): void {
  router.push({
    name: 'textexplore-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

function handleAutoSelectTable(table: SourceTable): void {
  router.replace({
    name: 'textexplore-table',
    params: { sourceId: props.sourceId, table: table.name },
  });
}

const {
  addTerm,
  commitInput,
  data,
  error,
  isLoading,
  loadMore,
  popTerm,
  removeTerm,
  terms,
  wholeWord,
} = useTextExplore(toRef(props, 'sourceId'), toRef(props, 'table'));
</script>

<template>
  <div class="flex h-full flex-col">
    <TableSectionPane
      enrichable-only
      :source-id="sourceId"
      :selected-table="table"
      @select-table="handleSelectTable"
      @auto-select-table="handleAutoSelectTable"
    />

    <div v-if="enrichableTables.length === 0" class="p-6">
      <p class="text-sm text-muted">
        Text Explorer needs a table with at least one text column. Sync or upload data to populate
        one.
      </p>
    </div>

    <div v-else-if="activeTable == null" class="p-6">
      <p class="text-sm text-muted">Choose a table above to explore its text.</p>
    </div>

    <div v-else class="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <TermInputBar
        v-model:whole-word="wholeWord"
        :terms="terms"
        @commit="commitInput"
        @remove="removeTerm"
        @pop="popTerm"
      />

      <p v-if="error" class="text-sm text-error">{{ error.message }}</p>

      <!-- First request builds the server-side index — can take a moment. -->
      <div v-else-if="data == null && isLoading" class="flex items-center gap-2 p-2">
        <UIcon name="i-lucide-loader-circle" class="animate-spin text-muted" />
        <span class="text-sm text-muted">Indexing table…</span>
      </div>

      <div v-else-if="data != null" class="flex min-h-0 flex-1 gap-6">
        <div class="min-w-0 flex-1 overflow-y-auto">
          <ResultsList
            :rows="data.rows"
            :matched-rows="data.matchedRows"
            :total-rows="data.totalRows"
            :loading="isLoading"
            @load-more="loadMore"
          />
        </div>
        <aside class="w-64 shrink-0 overflow-y-auto border-l border-default pl-6 lg:w-72">
          <WordsPanel
            :common="data.common"
            :distinctive="data.distinctive"
            @include="(term) => addTerm(term, false)"
            @exclude="(term) => addTerm(term, true)"
          />
        </aside>
      </div>
    </div>
  </div>
</template>

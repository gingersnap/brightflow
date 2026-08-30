<script setup lang="ts">
/**
 * Setup pane: the two ticket functions above, the vocabularies below. A
 * table has at most one function per kind; the card for a missing kind is
 * a create form. Everything else about a function — sample test, scoped
 * run, versions, delete — lives in FunctionCard.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed } from 'vue';

import { enrichFnApi, tableApi } from '@/services/api';
import type { EnrichFunction, FunctionKind } from '@/types/enrichment';

import FunctionCard from './FunctionCard.vue';
import VocabularyPanel from './VocabularyPanel.vue';

const props = defineProps<{
  sourceId: string;
  table: string;
}>();

const queryCache = useQueryCache();

const functionsKey = computed(() => ['enrich-fns', props.sourceId, props.table]);
const { data: functions } = useQuery({
  key: () => functionsKey.value,
  query: async () => (await enrichFnApi.list(props.sourceId, props.table)) ?? [],
});

const { data: tableIndex } = useQuery({
  key: () => ['tables-index', props.sourceId],
  query: async () => (await tableApi.listAvailable()) ?? [],
});

const columnNames = computed(() => {
  const info = (tableIndex.value ?? []).find(
    (t) => t.source_id === props.sourceId && t.name === props.table,
  );
  const schema = info?.schema as { fields?: { name?: string }[] } | null;
  return (schema?.fields ?? [])
    .map((f) => f.name)
    .filter((n): n is string => typeof n === 'string');
});

function byKind(kind: FunctionKind): EnrichFunction | null {
  return (functions.value ?? []).find((f) => f.kind === kind) ?? null;
}

async function refresh(): Promise<void> {
  await queryCache.invalidateQueries({ key: functionsKey.value });
}
</script>

<template>
  <div class="flex flex-col gap-8">
    <section class="flex flex-col gap-4">
      <div>
        <h3 class="text-sm font-medium text-highlighted">Ticket functions</h3>
        <p class="text-sm text-muted">
          Two LLM calls per ticket, run at ingestion: classification (what is this ticket?) and
          mention extraction (what is in it?). Test on a sample before running a table.
        </p>
      </div>
      <FunctionCard
        kind="ticket_classify"
        :fn="byKind('ticket_classify')"
        :column-names="columnNames"
        :source-id="sourceId"
        :table="table"
        @changed="refresh"
      />
      <FunctionCard
        kind="ticket_extract"
        :fn="byKind('ticket_extract')"
        :column-names="columnNames"
        :source-id="sourceId"
        :table="table"
        @changed="refresh"
      />
    </section>

    <VocabularyPanel :source-id="sourceId" :table="table" />
  </div>
</template>

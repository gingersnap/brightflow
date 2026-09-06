<script setup lang="ts">
/**
 * Classification tab: the classify function, the category → subcategory
 * tree it resolves against (with what it classified into each entry), and
 * the row-grain sentiment and language counts. Everything the classify call
 * reads or writes, and nothing the extract call does.
 */

import { useQuery } from '@pinia/colada';

import { ticketsApi } from '@/services/api';

import FunctionCard from './FunctionCard.vue';
import VocabularyTree from './VocabularyTree.vue';

const props = defineProps<{
  sourceId: string;
  table: string;
}>();

// Same key as VocabularyTree: one fetch serves the tree and this line.
const { data: tickets } = useQuery({
  key: () => ['tickets-summary', props.sourceId, props.table],
  query: () => ticketsApi.summary(props.sourceId, props.table),
});
</script>

<template>
  <div class="flex flex-col gap-8">
    <FunctionCard kind="ticket_classify" :source-id="sourceId" :table="table" />

    <VocabularyTree :source-id="sourceId" :table="table" :kinds="['category']" />

    <section v-if="tickets && tickets.classifiedRows > 0" class="flex flex-col gap-1">
      <h3 class="text-sm font-medium text-highlighted">Sentiment and language</h3>
      <div class="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted">
        <span v-for="s in tickets.sentiment" :key="s.value">
          {{ s.value }} {{ s.rows.toLocaleString() }}
        </span>
        <span class="text-dimmed">·</span>
        <span v-for="l in tickets.languages" :key="l.value">
          {{ l.value }} {{ l.rows.toLocaleString() }}
        </span>
      </div>
    </section>
  </div>
</template>

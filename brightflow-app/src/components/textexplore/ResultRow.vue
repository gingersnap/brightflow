<script setup lang="ts">
import type { TextExploreRow } from '@/types/generated';

import HighlightRuns from './HighlightRuns.vue';

defineProps<{
  row: TextExploreRow;
}>();

/** Date part of an ISO-ish timestamp for the metadata line. */
function datePart(ts: string): string {
  return ts.slice(0, 10);
}
</script>

<template>
  <article class="border-b border-default py-3 last:border-b-0">
    <h3 class="text-sm font-medium text-highlighted">
      <a
        v-if="row.htmlUrl != null"
        :href="row.htmlUrl"
        target="_blank"
        rel="noopener noreferrer"
        class="hover:underline"
      >
        <HighlightRuns :runs="row.title" />
      </a>
      <HighlightRuns v-else :runs="row.title" />
    </h3>

    <p v-if="row.number != null || row.timestamp != null" class="mt-0.5 text-xs text-muted">
      <template v-if="row.number != null">#{{ row.number }}</template>
      <template v-if="row.number != null && row.timestamp != null"> · </template>
      <template v-if="row.timestamp != null">{{ datePart(row.timestamp) }}</template>
    </p>

    <p v-if="row.snippet.length > 0" class="mt-1 text-sm text-muted">
      <HighlightRuns :runs="row.snippet" />
    </p>
  </article>
</template>

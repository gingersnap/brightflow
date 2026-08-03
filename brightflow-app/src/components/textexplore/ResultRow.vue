<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui';
import { useClipboard } from '@vueuse/core';
import { computed } from 'vue';

import type { TextExploreRow, TextRun } from '@/types/generated';

import HighlightRuns from './HighlightRuns.vue';

const props = defineProps<{
  row: TextExploreRow;
}>();

/** Date part of an ISO-ish timestamp for the metadata line. */
function datePart(ts: string): string {
  return ts.slice(0, 10);
}

/** Flatten pre-segmented highlight runs back to plain text (for clipboard). */
function runsToText(runs: TextRun[]): string {
  return runs.map((r) => r.t).join('');
}

const { copy } = useClipboard();

const items = computed<ContextMenuItem[][]>(() => {
  const title = runsToText(props.row.title);
  const snippet = runsToText(props.row.snippet);
  const group: ContextMenuItem[] = [
    { label: 'Copy title', icon: 'i-lucide-copy', onSelect: () => copy(title) },
  ];
  if (snippet.length > 0) {
    group.push({ label: 'Copy snippet', icon: 'i-lucide-copy', onSelect: () => copy(snippet) });
  }
  if (props.row.htmlUrl != null) {
    const url = props.row.htmlUrl;
    group.push(
      { label: 'Copy link', icon: 'i-lucide-link', onSelect: () => copy(url) },
      {
        label: 'Open in new tab',
        icon: 'i-lucide-external-link',
        onSelect: () => window.open(url, '_blank', 'noopener'),
      },
    );
  }
  return [group];
});
</script>

<template>
  <UContextMenu :items="items">
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
  </UContextMenu>
</template>

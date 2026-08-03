<script setup lang="ts">
/**
 * Search-result list with match counts and manual paging: "Load more"
 * shows only while fewer rows are loaded than matched, and the loading
 * spinner rides the count line so existing results stay visible during
 * refetches.
 */

import type { TextExploreRow } from '@/types/generated';

import ResultRow from './ResultRow.vue';

const props = defineProps<{
  rows: TextExploreRow[];
  matchedRows: number;
  totalRows: number;
  loading: boolean;
}>();

const emit = defineEmits<{
  loadMore: [];
}>();

function canLoadMore(): boolean {
  return props.rows.length < props.matchedRows;
}
</script>

<template>
  <div class="flex flex-col">
    <p class="text-sm text-muted">
      {{ matchedRows.toLocaleString() }} of {{ totalRows.toLocaleString() }} rows
      <UIcon v-if="loading" name="i-lucide-loader-circle" class="ml-1 inline-block animate-spin" />
    </p>

    <p v-if="rows.length === 0 && !loading" class="mt-4 text-sm text-muted">
      No rows match the current filters.
    </p>

    <div v-else class="mt-1">
      <ResultRow v-for="row in rows" :key="row.id" :row="row" />
    </div>

    <div v-if="canLoadMore()" class="mt-3">
      <UButton
        size="md"
        variant="subtle"
        color="neutral"
        :loading="loading"
        @click="emit('loadMore')"
      >
        Load more
      </UButton>
    </div>
  </div>
</template>

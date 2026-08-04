<script setup lang="ts">
/**
 * Topic-model draft editor: text-column selection plus the shared
 * TopicModelForm, mapped in and out of the wire-shaped TopicModelConfig
 * v-model. Persistence and lifecycle live in the shell.
 */

import { computed } from 'vue';

import TopicModelForm, { type TopicModelFormValue } from '@/components/topics/TopicModelForm.vue';
import type { TopicModelConfig } from '@/types/enrichment';

defineProps<{
  columnNames: string[];
  /** An existing function enables the clusters link. */
  hasFn: boolean;
  sourceId: string;
  table: string;
}>();

const config = defineModel<TopicModelConfig>('config', { required: true });

const form = computed<TopicModelFormValue>({
  get: () => ({
    algorithm: config.value.algorithm ?? 'kmeans',
    cleaningProfile: config.value.cleaning_profile ?? 'plain',
    languageColumn: config.value.language_column ?? '',
    minClusterSize: config.value.min_cluster_size ?? undefined,
  }),
  set: (value) => {
    config.value = {
      ...config.value,
      algorithm: value.algorithm || null,
      cleaning_profile: value.cleaningProfile || null,
      language_column: value.languageColumn || null,
      min_cluster_size: value.minClusterSize ?? null,
    };
  },
});

const textColumns = computed<string[]>({
  get: () => config.value.text_columns ?? [],
  set: (value) => {
    config.value = { ...config.value, text_columns: value.length > 0 ? value : null };
  },
});
</script>

<template>
  <div class="max-w-md space-y-3">
    <div>
      <p class="mb-1 text-sm font-medium text-highlighted">Text columns</p>
      <USelectMenu
        v-model="textColumns"
        :items="columnNames"
        multiple
        size="md"
        class="w-full"
        placeholder="Columns to embed (first = headline)"
      />
    </div>
    <TopicModelForm v-model="form" />
    <RouterLink
      v-if="hasFn"
      :to="{ name: 'topics-table', params: { sourceId, table } }"
      class="inline-flex items-center gap-1 text-sm text-primary-500 hover:underline"
    >
      View clusters
      <UIcon name="i-lucide-arrow-right" class="size-4" />
    </RouterLink>
  </div>
</template>

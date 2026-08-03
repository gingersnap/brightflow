<script setup lang="ts">
import { computed } from 'vue';

/**
 * Pure v-model form for topic-model settings (algorithm, min cluster size,
 * cleaning profile, language column). No fetching or saving here — the
 * parent owns load/save.
 */
export interface TopicModelFormValue {
  algorithm: string;
  minClusterSize: number | undefined;
  cleaningProfile: string;
  languageColumn: string;
}

const props = defineProps<{
  modelValue: TopicModelFormValue;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: TopicModelFormValue];
}>();

const algorithmItems = [
  { label: 'k-means (fixed k, every row assigned)', value: 'kmeans' },
  { label: 'HDBSCAN (density-based, finds k, flags noise)', value: 'hdbscan' },
];

const profileItems = [
  { label: 'Social (URLs, @handles, #tags, emoji)', value: 'social' },
  { label: 'Markdown issue (code fences, templates)', value: 'markdown_issue' },
  { label: 'Plain (whitespace only)', value: 'plain' },
];

function field<K extends keyof TopicModelFormValue>(key: K) {
  return computed({
    get: () => props.modelValue[key],
    set: (value: TopicModelFormValue[K]) =>
      emit('update:modelValue', { ...props.modelValue, [key]: value }),
  });
}

const algorithm = field('algorithm');
const minClusterSize = field('minClusterSize');
const cleaningProfile = field('cleaningProfile');
const languageColumn = field('languageColumn');
</script>

<template>
  <div class="space-y-3">
    <div>
      <p class="mb-1 text-sm font-medium text-highlighted">Clustering algorithm</p>
      <USelect v-model="algorithm" :items="algorithmItems" size="md" class="w-full" />
    </div>
    <div>
      <p class="mb-1 text-sm font-medium text-highlighted">Min cluster size</p>
      <UInput
        v-model.number="minClusterSize"
        type="number"
        placeholder="auto"
        size="md"
        class="w-full"
      />
    </div>
    <div>
      <p class="mb-1 text-sm font-medium text-highlighted">Cleaning profile</p>
      <USelect v-model="cleaningProfile" :items="profileItems" size="md" class="w-full" />
      <p class="mt-1 text-sm text-muted">
        Switching lazily re-embeds all rows on the next recluster.
      </p>
    </div>
    <div>
      <p class="mb-1 text-sm font-medium text-highlighted">Language column</p>
      <UInput v-model="languageColumn" placeholder="auto-detect" size="md" class="w-full" />
    </div>
  </div>
</template>

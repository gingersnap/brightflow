<script setup lang="ts">
import { computed } from 'vue';

import type { TopicsOverview } from '@/types/generated';

const props = defineProps<{
  overview: TopicsOverview | null | undefined;
  isLoading: boolean;
  isReclustering: boolean;
  kInput: number | undefined;
}>();

const emit = defineEmits<{
  recluster: [];
  'update:kInput': [value: number | undefined];
}>();

const fittedAt = computed(() => {
  const ts = props.overview?.fittedAt;
  if (ts == null) {
    return null;
  }
  return new Date(Number(ts) * 1000).toLocaleString();
});

const totalRows = computed(() => props.overview?.totalRows ?? 0);
const k = computed(() => props.overview?.k ?? null);
const modelId = computed(() => props.overview?.embeddingModelId ?? null);

function handleKInput(event: Event): void {
  const target = event.target as HTMLInputElement;
  const value = target.value.trim();
  if (value === '') {
    emit('update:kInput');
    return;
  }
  const n = Number.parseInt(value, 10);
  emit('update:kInput', Number.isNaN(n) ? undefined : n);
}
</script>

<template>
  <div class="border-b border-default bg-elevated px-6 py-4">
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <h1 class="text-base font-semibold text-highlighted">Topics</h1>
        <div v-if="overview?.ready" class="flex flex-wrap items-center gap-2 text-sm text-muted">
          <span>{{ totalRows.toLocaleString() }} rows</span>
          <span v-if="k != null">·</span>
          <span v-if="k != null">k = {{ k }}</span>
          <span v-if="fittedAt">·</span>
          <span v-if="fittedAt">fitted {{ fittedAt }}</span>
          <span v-if="modelId">·</span>
          <code v-if="modelId" class="text-xs">{{ modelId }}</code>
        </div>
        <span v-else-if="isLoading" class="text-sm text-muted">Loading…</span>
        <span v-else class="text-sm text-muted">No clusters fitted yet.</span>
      </div>

      <div class="flex items-center gap-2">
        <UInput
          type="number"
          placeholder="k = 10"
          :model-value="kInput"
          size="md"
          class="w-24"
          @input="handleKInput"
        />
        <UButton
          size="md"
          color="primary"
          icon="i-lucide-shuffle"
          :loading="isReclustering"
          :disabled="isReclustering"
          @click="emit('recluster')"
        >
          Recluster
        </UButton>
      </div>
    </div>
  </div>
</template>

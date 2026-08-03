<script setup lang="ts">
/**
 * Header bar for the Topics view: a dense one-line summary of the current
 * fit (rows, k, fit time, language mix, unassigned/hidden counts, embedding
 * model) next to the recluster controls — k override, recluster button, and
 * the enrichment-settings popover. Emits only; the parent runs the recluster.
 */

import { computed } from 'vue';

import type { TopicsOverview } from '@/types/generated';

import EnrichmentSettingsPanel from './EnrichmentSettingsPanel.vue';

const props = defineProps<{
  overview: TopicsOverview | null | undefined;
  isLoading: boolean;
  isReclustering: boolean;
  kInput: number | undefined;
  sourceId: string;
  table: string;
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
const unassignedRows = computed(() => props.overview?.unassignedRows ?? 0);
const hiddenClusters = computed(() => props.overview?.hiddenClusters ?? 0);
const language = computed(() => props.overview?.language ?? null);
const otherLanguages = computed(() => {
  const hist = props.overview?.languageHistogram ?? [];
  const lang = language.value;
  return hist.filter((b) => b.language !== lang).reduce((acc, b) => acc + b.count, 0);
});

function handleKInput(event: Event): void {
  const target = event.target as HTMLInputElement;
  const value = target.value.trim();
  if (value === '') {
    emit('update:kInput');
    return;
  }
  const n = Math.trunc(Number(value));
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
          <span v-if="language">·</span>
          <span v-if="language">
            {{ language }}
            <template v-if="otherLanguages > 0">
              (+{{ otherLanguages.toLocaleString() }} rows in other languages)
            </template>
          </span>
          <span v-if="unassignedRows > 0">·</span>
          <span v-if="unassignedRows > 0"> {{ unassignedRows.toLocaleString() }} unassigned </span>
          <span v-if="hiddenClusters > 0">·</span>
          <span v-if="hiddenClusters > 0">{{ hiddenClusters }} small clusters hidden</span>
          <span v-if="modelId">·</span>
          <code v-if="modelId" class="text-xs">{{ modelId }}</code>
        </div>
        <span v-else-if="isLoading" class="text-sm text-muted">Loading…</span>
        <span v-else class="text-sm text-muted">No clusters fitted yet.</span>
      </div>

      <div class="flex items-center gap-2">
        <UPopover>
          <UButton
            size="md"
            color="neutral"
            variant="soft"
            icon="i-lucide-settings-2"
            aria-label="Enrichment settings"
          />
          <template #content>
            <EnrichmentSettingsPanel :source-id="sourceId" :table="table" />
          </template>
        </UPopover>
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

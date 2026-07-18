<script setup lang="ts">
import { onMounted, ref } from 'vue';

import { topicsApi } from '@/services/api';

import TopicModelForm, { type TopicModelFormValue } from './TopicModelForm.vue';

/**
 * Per-table enrichment settings — the GUI twin of
 * GET/PUT /api/sources/{id}/tables/{table}/enrichment. Thin wrapper around
 * TopicModelForm (shared with the Enrich function editor): this panel owns
 * load/save, the form owns the fields.
 */
const props = defineProps<{
  sourceId: string;
  table: string;
}>();

const emit = defineEmits<{
  saved: [];
}>();

const loading = ref(true);
const saving = ref(false);
const error = ref<string | null>(null);
const savedTick = ref(false);

const form = ref<TopicModelFormValue>({
  algorithm: 'kmeans',
  cleaningProfile: '',
  languageColumn: '',
  minClusterSize: undefined,
});

onMounted(async () => {
  loading.value = true;
  try {
    const settings = await topicsApi.getEnrichment(props.sourceId, props.table);
    if (settings != null && settings.enrichable) {
      form.value = {
        algorithm: settings.algorithm || 'kmeans',
        cleaningProfile: settings.cleaningProfile,
        languageColumn: settings.languageColumn ?? '',
        minClusterSize: settings.minClusterSize ?? undefined,
      };
    }
  } finally {
    loading.value = false;
  }
});

async function save(): Promise<void> {
  saving.value = true;
  error.value = null;
  savedTick.value = false;
  try {
    const result = await topicsApi.updateEnrichment(props.sourceId, props.table, {
      algorithm: form.value.algorithm,
      ...(form.value.minClusterSize == null ? {} : { minClusterSize: form.value.minClusterSize }),
      cleaningProfile: form.value.cleaningProfile,
      languageColumn: form.value.languageColumn,
    });
    if (result != null) {
      savedTick.value = true;
      emit('saved');
    }
    // oxlint-disable-next-line unicorn/catch-error-name -- error shadows ref
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Save failed';
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="w-80 space-y-3 p-3">
    <p v-if="loading" class="text-sm text-muted">Loading…</p>
    <template v-else>
      <TopicModelForm v-model="form" />
      <p v-if="error" class="text-sm text-red-500">{{ error }}</p>
      <div class="flex items-center gap-2">
        <UButton size="md" color="primary" :loading="saving" @click="save">Save</UButton>
        <span v-if="savedTick" class="text-sm text-muted">Saved — recluster to apply</span>
      </div>
    </template>
  </div>
</template>

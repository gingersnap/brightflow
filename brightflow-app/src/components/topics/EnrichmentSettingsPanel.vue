<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';

import { topicsApi } from '@/services/api';

/**
 * Per-table enrichment settings — the GUI twin of
 * GET/PUT /api/sources/{id}/tables/{table}/enrichment. Everything settable
 * via the API is settable here; nothing is CLI/API-only.
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

const form = reactive({
  algorithm: 'kmeans',
  minClusterSize: undefined as number | undefined,
  cleaningProfile: '',
  languageColumn: '',
});

const algorithmItems = [
  { label: 'k-means (fixed k, every row assigned)', value: 'kmeans' },
  { label: 'HDBSCAN (density-based, finds k, flags noise)', value: 'hdbscan' },
];

const profileItems = [
  { label: 'Social (URLs, @handles, #tags, emoji)', value: 'social' },
  { label: 'Markdown issue (code fences, templates)', value: 'markdown_issue' },
  { label: 'Plain (whitespace only)', value: 'plain' },
];

onMounted(async () => {
  loading.value = true;
  try {
    const settings = await topicsApi.getEnrichment(props.sourceId, props.table);
    if (settings != null && settings.enrichable) {
      form.algorithm = settings.algorithm || 'kmeans';
      form.minClusterSize = settings.minClusterSize ?? undefined;
      form.cleaningProfile = settings.cleaningProfile;
      form.languageColumn = settings.languageColumn ?? '';
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
      algorithm: form.algorithm,
      ...(form.minClusterSize == null ? {} : { minClusterSize: form.minClusterSize }),
      cleaningProfile: form.cleaningProfile,
      languageColumn: form.languageColumn,
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
      <div>
        <p class="mb-1 text-sm font-medium text-highlighted">Clustering algorithm</p>
        <USelect v-model="form.algorithm" :items="algorithmItems" size="md" class="w-full" />
      </div>
      <div>
        <p class="mb-1 text-sm font-medium text-highlighted">Min cluster size</p>
        <UInput
          v-model.number="form.minClusterSize"
          type="number"
          placeholder="auto"
          size="md"
          class="w-full"
        />
      </div>
      <div>
        <p class="mb-1 text-sm font-medium text-highlighted">Cleaning profile</p>
        <USelect v-model="form.cleaningProfile" :items="profileItems" size="md" class="w-full" />
        <p class="mt-1 text-sm text-muted">
          Switching lazily re-embeds all rows on the next recluster.
        </p>
      </div>
      <div>
        <p class="mb-1 text-sm font-medium text-highlighted">Language column</p>
        <UInput v-model="form.languageColumn" placeholder="auto-detect" size="md" class="w-full" />
      </div>
      <p v-if="error" class="text-sm text-red-500">{{ error }}</p>
      <div class="flex items-center gap-2">
        <UButton size="md" color="primary" :loading="saving" @click="save">Save</UButton>
        <span v-if="savedTick" class="text-sm text-muted">Saved — recluster to apply</span>
      </div>
    </template>
  </div>
</template>

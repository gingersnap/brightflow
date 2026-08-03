<script setup lang="ts">
import { useQueryCache } from '@pinia/colada';
import { ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import { sourceApi } from '@/services/api';

import { previewCsv, slugifyTableName, type CsvPreview } from './csvPreview';

const router = useRouter();
const queryCache = useQueryCache();

const file = ref<File | null>(null);
const tableName = ref('');
const preview = ref<CsvPreview | null>(null);
const submitting = ref(false);
const errorMessage = ref<string | null>(null);

watch(file, async (selected) => {
  preview.value = null;
  errorMessage.value = null;
  if (selected == null) {
    return;
  }
  tableName.value = slugifyTableName(selected.name);
  // Preview from the first 64KB — enough for a handful of rows.
  const head = await selected.slice(0, 64 * 1024).text();
  preview.value = previewCsv(head, 5);
});

async function submit(): Promise<void> {
  const selected = file.value;
  if (selected == null) {
    return;
  }
  submitting.value = true;
  errorMessage.value = null;
  try {
    const result = await sourceApi.uploadCsv(selected, tableName.value);
    queryCache.invalidateQueries({ key: ['unified-sources'] });
    await router.push({
      name: 'explore-table',
      params: { sourceId: result.sourceId, table: result.table },
    });
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : 'Upload failed';
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <section class="space-y-4 rounded-lg border border-default bg-elevated p-4">
    <div v-if="errorMessage" class="rounded bg-red-500/10 p-2 text-sm text-red-500">
      {{ errorMessage }}
    </div>

    <UFileUpload v-model="file" accept=".csv,text/csv" label="Drop a CSV here" class="w-full" />

    <template v-if="file">
      <div>
        <label class="mb-1 block text-sm font-medium text-muted">Table name</label>
        <UInput v-model="tableName" size="md" class="w-64 font-mono" />
      </div>

      <div v-if="preview && preview.headers.length > 0" class="overflow-x-auto">
        <p class="mb-1 text-sm text-muted">Preview (first {{ preview.rows.length }} rows)</p>
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b border-default text-left text-muted">
              <th v-for="h in preview.headers" :key="h" class="px-2 py-1.5 font-medium">
                {{ h }}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, i) in preview.rows" :key="i" class="border-b border-default/50">
              <td v-for="(cell, j) in row" :key="j" class="max-w-48 truncate px-2 py-1.5">
                {{ cell }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div class="flex justify-end pt-1">
        <UButton
          size="md"
          color="primary"
          :loading="submitting"
          :disabled="tableName.trim() === ''"
          icon="i-lucide-upload"
          @click="submit"
        >
          Upload as source
        </UButton>
      </div>
    </template>

    <p class="text-sm text-muted">
      Uploads become a persistent source: the data survives restarts and works with Explore,
      Insights, Topics and Enrich.
    </p>
  </section>
</template>

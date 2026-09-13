<script setup lang="ts">
/**
 * The source's semantic model, in and out, at the foot of the Semantics
 * page. "Export" fetches the Ossie document the store rebuilds from its
 * resolved rows and shows it in the editor; a pasted document (or a bare
 * model in the short hand-authored form) is checked with a dry run and
 * applied as declared rows under `document:{name}`, one table at a time.
 * Datasets with no table here are listed, not refused; a malformed model
 * is refused with the reason. Source-scoped, so it needs no table picked.
 */

import { ref } from 'vue';

import { ApiError, semanticModelApi } from '@/services/api';
import type { SemanticModelImportResponse } from '@/types/generated';

const props = defineProps<{ sourceId: string }>();

const text = ref('');
const busy = ref(false);
const failure = ref<string | null>(null);
const result = ref<SemanticModelImportResponse | null>(null);

function parseInput(): unknown | null {
  failure.value = null;
  try {
    return JSON.parse(text.value) as unknown;
  } catch (error) {
    failure.value = `Not JSON: ${error instanceof Error ? error.message : String(error)}`;
    return null;
  }
}

async function run(dryRun: boolean): Promise<void> {
  const document = parseInput();
  if (document == null) {
    return;
  }
  busy.value = true;
  result.value = null;
  try {
    result.value = await semanticModelApi.import(props.sourceId, document, dryRun);
  } catch (error) {
    failure.value = error instanceof ApiError ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

async function exportModel(): Promise<void> {
  busy.value = true;
  failure.value = null;
  result.value = null;
  try {
    const exported = await semanticModelApi.export(props.sourceId);
    text.value = exported == null ? '' : JSON.stringify(exported, null, 2);
  } catch (error) {
    failure.value = error instanceof ApiError ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

function summary(entry: SemanticModelImportResponse['applied'][number]): string {
  const parts = [`${entry.columns} columns`];
  if (entry.relationships > 0) {
    parts.push(`${entry.relationships} relationships`);
  }
  if (entry.metrics > 0) {
    parts.push(`${entry.metrics} metrics`);
  }
  return parts.join(', ');
}
</script>

<template>
  <section>
    <div class="space-y-3 rounded-lg border border-default bg-elevated p-4">
      <p class="text-sm text-muted">
        What this source's tables mean, as an Ossie document. Export the current model to edit it,
        or paste one and apply it; a person's edits in Explore still win over it.
      </p>
      <UTextarea
        v-model="text"
        :rows="12"
        autoresize
        class="w-full font-mono text-sm"
        placeholder='{ "name": "...", "datasets": [ ... ] }'
      />
      <div class="flex flex-wrap gap-2">
        <UButton size="md" variant="ghost" :loading="busy" @click="exportModel()">
          <UIcon name="i-lucide-download" class="mr-1.5 h-3.5 w-3.5" />
          Export current
        </UButton>
        <UButton
          size="md"
          variant="soft"
          :loading="busy"
          :disabled="text === ''"
          @click="run(true)"
        >
          Check
        </UButton>
        <UButton size="md" :loading="busy" :disabled="text === ''" @click="run(false)">
          Apply
        </UButton>
      </div>
      <p v-if="failure" class="text-sm text-error">{{ failure }}</p>
      <div v-if="result" class="space-y-1 text-sm">
        <p class="text-highlighted">
          {{ result.dryRun ? 'Would apply' : 'Applied' }} as {{ result.producer }}
        </p>
        <ul class="space-y-1">
          <li v-for="entry in result.applied" :key="entry.table">
            <span class="font-medium">{{ entry.table }}</span>
            <span class="text-muted"> — {{ summary(entry) }}</span>
            <span v-if="entry.columnsWithoutData.length > 0" class="text-muted">
              ; no data for {{ entry.columnsWithoutData.join(', ') }}
            </span>
            <span v-if="entry.metricsSkipped.length > 0" class="text-muted">
              ; skipped metrics {{ entry.metricsSkipped.join(', ') }}
            </span>
            <span v-if="entry.relationshipsSkipped.length > 0" class="text-muted">
              ; relationships to missing tables {{ entry.relationshipsSkipped.join(', ') }}
            </span>
          </li>
        </ul>
        <p v-if="result.missingTables.length > 0" class="text-muted">
          No table here for {{ result.missingTables.join(', ') }}
        </p>
      </div>
    </div>
  </section>
</template>

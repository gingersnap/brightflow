<script setup lang="ts">
/**
 * LLM-prompt draft editor: the prompt form plus the test-on-sample loop.
 * Owns nothing about persistence — the config is a v-model the shell saves,
 * and `restoreConfig` (exposed) normalizes a version-history payload back
 * into it. "Run all" is only requested upward; the shell owns the modal.
 */

import { computed, ref } from 'vue';

import { useSampleRun } from '@/composables/useSampleRun';
import type { EnrichFunction, LlmPromptConfig } from '@/types/enrichment';

import LlmPromptForm from './LlmPromptForm.vue';

const props = defineProps<{
  columnNames: string[];
  /** Existing function (enables the sample loop), or null while creating. */
  fn: EnrichFunction | null;
  /** Unsaved edits gate "Run all" — the run executes the saved version. */
  isDirty: boolean;
}>();

const emit = defineEmits<{
  runAll: [];
}>();

const config = defineModel<LlmPromptConfig>('config', { required: true });

/** Version-history restore: fill defaults so a partial payload can't leave holes. */
function restoreConfig(restored: Partial<LlmPromptConfig>): void {
  config.value = {
    input_columns: restored.input_columns ?? [],
    model: restored.model ?? null,
    outputs: restored.outputs ?? [],
    prompt_template: restored.prompt_template ?? '',
    provider_id: restored.provider_id ?? 'default',
  };
}

defineExpose({ restoreConfig });

// ---------------------------------------------------------------------------
// Sample loop
// ---------------------------------------------------------------------------

const sample = useSampleRun(() => props.fn?.id ?? null);
const lastTestedConfig = ref('');
const compareOpen = ref(false);

const needsRetest = computed(
  () => sample.runCount.value > 0 && JSON.stringify(config.value) !== lastTestedConfig.value,
);

async function testSample(limit?: number): Promise<void> {
  lastTestedConfig.value = JSON.stringify(config.value);
  await sample.run({ ...config.value }, limit);
}

async function widenSample(): Promise<void> {
  lastTestedConfig.value = JSON.stringify(config.value);
  await sample.widen({ ...config.value });
}

const outputNames = computed(() => config.value.outputs.map((o) => o.name).filter(Boolean));
const inputNames = computed(() => {
  const first = sample.rows.value[0];
  return first ? Object.keys(first.inputs) : [];
});

function cellValue(row: Record<string, unknown> | null | undefined, name: string): string {
  const value = row?.[name];
  if (value == null) {
    return '';
  }
  return typeof value === 'string' ? value : JSON.stringify(value);
}
</script>

<template>
  <LlmPromptForm v-model="config" :columns="columnNames" />

  <!-- Sample loop -->
  <div v-if="fn" class="space-y-3 rounded-lg border border-default p-3">
    <div class="flex flex-wrap items-center gap-2">
      <p class="text-sm font-medium text-highlighted">Test on sample</p>
      <span v-if="needsRetest" class="text-sm text-warning">Prompt changed — re-test</span>
      <div class="ml-auto flex items-center gap-2">
        <UButton
          size="md"
          color="neutral"
          variant="soft"
          icon="i-lucide-flask-conical"
          :loading="sample.loading.value"
          @click="testSample(10)"
        >
          {{ sample.runCount.value === 0 ? 'Test 10 rows' : 'Re-test' }}
        </UButton>
        <UButton
          v-if="sample.runCount.value > 0"
          size="md"
          color="neutral"
          variant="soft"
          icon="i-lucide-expand"
          :loading="sample.loading.value"
          @click="widenSample"
        >
          Widen to 100
        </UButton>
        <UButton
          v-if="fn"
          size="md"
          color="primary"
          variant="solid"
          icon="i-lucide-play"
          :disabled="isDirty"
          :title="isDirty ? 'Save first' : undefined"
          @click="emit('runAll')"
        >
          Run all
        </UButton>
      </div>
    </div>
    <p v-if="sample.errorMessage.value" class="text-sm text-error">
      {{ sample.errorMessage.value }}
    </p>

    <template v-if="sample.rows.value.length > 0">
      <div class="flex items-center gap-3 text-sm text-muted">
        <span>
          {{ sample.rows.value.length }} rows · {{ sample.result.value?.totalTokens ?? 0 }} tokens ·
          {{ sample.result.value?.cacheHits ?? 0 }} cached
        </span>
        <UButton
          v-if="sample.previousRows.value.length > 0"
          size="xs"
          color="neutral"
          :variant="compareOpen ? 'soft' : 'ghost'"
          icon="i-lucide-git-compare"
          @click="compareOpen = !compareOpen"
        >
          Compare with previous
        </UButton>
      </div>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b border-default text-left text-muted">
              <th v-for="name in inputNames" :key="name" class="px-2 py-1.5 font-medium">
                {{ name }}
              </th>
              <th
                v-for="name in outputNames"
                :key="name"
                class="px-2 py-1.5 font-medium text-primary-500"
              >
                {{ name }}
              </th>
              <th v-if="compareOpen" class="px-2 py-1.5 font-medium">previous</th>
              <th class="w-16 px-2 py-1.5" />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in sample.rows.value"
              :key="row.rowKey"
              class="border-b border-default/50 align-top"
              :class="{ 'bg-primary-500/5': sample.changedKeys.value.has(row.rowKey) }"
            >
              <td v-for="name in inputNames" :key="name" class="max-w-64 px-2 py-1.5">
                <span class="line-clamp-2 text-muted">{{ row.inputs[name] }}</span>
              </td>
              <td v-for="name in outputNames" :key="name" class="max-w-56 px-2 py-1.5">
                <span v-if="row.status === 'ok'" class="text-default">
                  {{ cellValue(row.value, name) }}
                </span>
                <span v-else class="text-error" :title="row.error ?? undefined">error</span>
              </td>
              <td v-if="compareOpen" class="max-w-56 px-2 py-1.5">
                <span v-for="name in outputNames" :key="name" class="block text-muted line-through">
                  {{ cellValue(sample.previousByKey.value.get(row.rowKey)?.value, name) }}
                </span>
              </td>
              <td class="px-2 py-1.5">
                <UBadge
                  v-if="sample.showCacheBadges.value"
                  :color="row.cached ? 'neutral' : 'primary'"
                  variant="subtle"
                  size="sm"
                >
                  {{ row.cached ? 'cached' : 'new' }}
                </UBadge>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
  <p v-else class="text-sm text-muted">
    Create the draft first, then test it on 10 rows before running the whole table.
  </p>
</template>

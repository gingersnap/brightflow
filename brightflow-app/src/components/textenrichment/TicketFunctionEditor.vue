<script setup lang="ts">
/**
 * Editor for the two built-in ticket kinds (`ticket_classify`, Call A, and
 * `ticket_extract`, Call B). The prompt is owned by the server and the
 * vocabulary snapshot is injected there; the only client-side config is
 * which text columns to read, an optional source language column, and the
 * provider. Both kinds are promoted from birth, so there is no draft loop,
 * but the sample run is the gate before a full one: a full run is one LLM
 * call per row, and tables run to tens of thousands of rows.
 */

import { useQuery } from '@pinia/colada';
import { computed } from 'vue';

import { useSampleRun } from '@/composables/useSampleRun';
import { llmApi } from '@/services/api';
import type { EnrichFunction, FunctionKind, TicketFunctionConfig } from '@/types/enrichment';

const props = defineProps<{
  columnNames: string[];
  fn: EnrichFunction | null;
  kind: FunctionKind;
  sourceId: string;
  table: string;
}>();

const emit = defineEmits<{
  runAll: [];
}>();

const config = defineModel<TicketFunctionConfig>('config', { required: true });

const { data: providers } = useQuery({
  key: ['llm-providers'],
  query: async () => (await llmApi.listProviders()) ?? [],
});

const providerItems = computed(() =>
  (providers.value ?? []).map((p) => ({
    label: `${p.name} (${p.model})`,
    value: String(p.id),
  })),
);

const textColumns = computed<string[]>({
  get: () => config.value.text_columns,
  set: (value) => {
    config.value = { ...config.value, text_columns: value };
  },
});

/** '' = detect language pre-call; a column = trust the source's tag. */
const languageColumn = computed<string>({
  get: () => config.value.language_column ?? '',
  set: (value) => {
    config.value = { ...config.value, language_column: value === '' ? null : value };
  },
});

const languageItems = computed(() => [
  { label: 'Detect from text', value: '' },
  ...props.columnNames.map((c) => ({ label: c, value: c })),
]);

const providerId = computed<string>({
  get: () => config.value.provider_id,
  set: (value) => {
    config.value = { ...config.value, provider_id: value };
  },
});

const model = computed<string>({
  get: () => config.value.model ?? '',
  set: (value) => {
    config.value = { ...config.value, model: value.trim() === '' ? null : value.trim() };
  },
});

const isClassify = computed(() => props.kind === 'ticket_classify');

// ---------------------------------------------------------------------------
// Sample loop (saved config only — the server injects the vocabulary)
// ---------------------------------------------------------------------------

const sample = useSampleRun(() => props.fn?.id ?? null);

/** Rendered input keys, with the runner's derived language shown by name. */
const inputNames = computed(() => {
  const first = sample.rows.value[0];
  return first ? Object.keys(first.inputs) : [];
});

function inputLabel(name: string): string {
  return name === '__language' ? 'language' : name;
}

const previewNames = computed(() =>
  isClassify.value ? ['summary', 'category', 'subcategory', 'sentiment'] : ['mentions'],
);

function cellValue(row: Record<string, unknown> | null | undefined, name: string): string {
  const value = row?.[name];
  if (value == null) {
    return '';
  }
  if (name === 'mentions' && Array.isArray(value)) {
    if (value.length === 0) {
      return '— none —';
    }
    return value
      .map((m) => {
        const mention = m as Record<string, unknown>;
        const what = mention['feedback_summary'] ?? mention['subject'] ?? '';
        const tag = mention['incidental'] === true ? ' (incidental)' : '';
        return `${String(mention['type'])}: ${String(what)}${tag}`;
      })
      .join('\n');
  }
  return typeof value === 'string' ? value : JSON.stringify(value);
}

const outputs = computed(() =>
  isClassify.value
    ? ['summary', 'language', 'category', 'subcategory', 'sentiment']
    : ['has_feedback', 'has_incidental_feedback', 'has_competitor_mention', 'mention_count'],
);
</script>

<template>
  <div class="max-w-lg space-y-4">
    <p class="text-sm text-muted">
      <template v-if="isClassify">
        Summarises and classifies every row: an English summary, the detected language, a category
        and subcategory from this table's vocabulary, and sentiment. Writes
        <code class="font-mono">{{ outputs.join(', ') }}</code
        >.
      </template>
      <template v-else>
        Extracts every product, competitor, pricing, service and feedback mention into
        <code class="font-mono">{{ table }}_mentions</code> and writes
        <code class="font-mono">{{ outputs.join(', ') }}</code> on each row.
      </template>
    </p>

    <div>
      <p class="mb-1 text-sm font-medium text-highlighted">Text columns</p>
      <USelectMenu
        v-model="textColumns"
        :items="columnNames"
        multiple
        size="md"
        class="w-full"
        placeholder="Columns the model reads (first = headline)"
      />
    </div>

    <div>
      <p class="mb-1 text-sm font-medium text-highlighted">Language</p>
      <USelectMenu
        v-model="languageColumn"
        :items="languageItems"
        value-key="value"
        size="md"
        class="w-full"
      />
      <p class="mt-1 text-sm text-muted">
        Language is decided before the call, never by the model — it drives per-language cost and
        launch gating.
      </p>
    </div>

    <div class="grid grid-cols-2 gap-3">
      <div>
        <p class="mb-1 text-sm font-medium text-highlighted">Provider</p>
        <USelectMenu
          v-model="providerId"
          :items="providerItems"
          value-key="value"
          size="md"
          class="w-full"
          placeholder="Default provider"
        />
      </div>
      <div>
        <p class="mb-1 text-sm font-medium text-highlighted">Model override</p>
        <UInput v-model="model" placeholder="provider default" size="md" class="w-full" />
      </div>
    </div>
    <p v-if="providerItems.length === 0" class="text-sm text-warning">
      No LLM provider configured — add one under Preferences → LLM first.
    </p>

    <div v-if="fn" class="space-y-3 rounded-lg border border-default p-3">
      <div class="flex flex-wrap items-center gap-2">
        <p class="text-sm font-medium text-highlighted">Test on sample</p>
        <div class="ml-auto flex items-center gap-2">
          <UButton
            size="md"
            color="neutral"
            variant="soft"
            icon="i-lucide-flask-conical"
            :loading="sample.loading.value"
            @click="() => void sample.run(null, 10)"
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
            @click="() => void sample.widen(null)"
          >
            Widen to 100
          </UButton>
        </div>
      </div>
      <p v-if="sample.errorMessage.value" class="text-sm text-error">
        {{ sample.errorMessage.value }}
      </p>
      <template v-if="sample.rows.value.length > 0">
        <p class="text-sm text-muted">
          {{ sample.rows.value.length }} rows · {{ sample.result.value?.totalTokens ?? 0 }} tokens
          ({{ sample.result.value?.cachedTokens ?? 0 }} served from the provider cache) ·
          {{ sample.result.value?.cacheHits ?? 0 }} cells cached locally
        </p>
        <div class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-default text-left text-muted">
                <th v-for="name in inputNames" :key="name" class="px-2 py-1.5 font-medium">
                  {{ inputLabel(name) }}
                </th>
                <th
                  v-for="name in previewNames"
                  :key="name"
                  class="px-2 py-1.5 font-medium text-primary-500"
                >
                  {{ name }}
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="row in sample.rows.value"
                :key="row.rowKey"
                class="border-b border-default/50 align-top"
              >
                <td v-for="name in inputNames" :key="name" class="max-w-64 px-2 py-1.5">
                  <span class="line-clamp-2 text-muted">{{ row.inputs[name] }}</span>
                </td>
                <td
                  v-for="name in previewNames"
                  :key="name"
                  class="max-w-72 px-2 py-1.5 whitespace-pre-line"
                >
                  <span v-if="row.status === 'ok'" class="text-default">
                    {{ cellValue(row.value, name) }}
                  </span>
                  <span v-else class="text-error" :title="row.error ?? undefined">
                    error: {{ row.error }}
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </div>

    <div v-if="fn" class="flex items-center gap-3 border-t border-default pt-4">
      <UButton
        size="md"
        color="primary"
        variant="soft"
        icon="i-lucide-play"
        @click="emit('runAll')"
      >
        Run all
      </UButton>
      <span class="text-sm text-muted">
        One LLM call per uncached row<template v-if="(fn.staleRowCount ?? 0) > 0">
          — {{ fn.staleRowCount?.toLocaleString() }} rows not yet computed</template
        >
      </span>
      <!-- Was 'topics-table', a route removed with the clustering path. An
           unresolvable name makes RouterLink throw on every render, so this
           now targets the tool that hosts the vocabularies. Note it resolves
           to the page this editor already sits on — see the note in the
           review if this link should scroll or simply go. -->
      <RouterLink
        :to="{ name: 'textenrichment-table', params: { sourceId, table } }"
        class="ml-auto inline-flex items-center gap-1 text-sm text-primary-500 hover:underline"
      >
        Vocabulary
        <UIcon name="i-lucide-arrow-right" class="size-4" />
      </RouterLink>
    </div>
  </div>
</template>

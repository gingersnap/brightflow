<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';

import TopicModelForm, { type TopicModelFormValue } from '@/components/topics/TopicModelForm.vue';
import { useEnrichRun } from '@/composables/useEnrichRun';
import { useSampleRun } from '@/composables/useSampleRun';
import { enrichFnApi } from '@/services/api';
import type {
  EnrichFunction,
  FunctionKind,
  LlmPromptConfig,
  RunScope,
  TopicModelConfig,
} from '@/types/enrichment';

import ActiveRunBar from './ActiveRunBar.vue';
import LlmPromptForm from './LlmPromptForm.vue';
import PromoteToggle from './PromoteToggle.vue';
import RerunPromptModal from './RerunPromptModal.vue';
import RunAllModal from './RunAllModal.vue';
import VersionHistoryPanel from './VersionHistoryPanel.vue';

const props = defineProps<{
  sourceId: string;
  table: string;
  columns: { name: string; dtype: string }[];
  /** Existing function to edit, or null when creating (see createKind). */
  fn: EnrichFunction | null;
  createKind: FunctionKind | null;
}>();

const emit = defineEmits<{
  saved: [id: string];
  deleted: [];
}>();

const columnNames = computed(() => props.columns.map((c) => c.name));
const kind = computed<FunctionKind>(() => props.fn?.kind ?? props.createKind ?? 'llm_prompt');
const isNew = computed(() => props.fn == null);

// ---------------------------------------------------------------------------
// Draft state (the editor owns it; isDirty gates save + re-test hints)
// ---------------------------------------------------------------------------

const draftName = ref('');
const llmDraft = ref<LlmPromptConfig>(emptyLlmConfig());
const topicDraft = ref<TopicModelFormValue>(emptyTopicForm());
const topicTextColumns = ref<string[]>([]);
const savedSnapshot = ref('');
const saving = ref(false);
const saveError = ref<string | null>(null);

function emptyLlmConfig(): LlmPromptConfig {
  return {
    input_columns: [],
    model: null,
    outputs: [{ description: '', dtype: { type: 'string' }, name: '' }],
    prompt_template: '',
    provider_id: 'default',
  };
}

function emptyTopicForm(): TopicModelFormValue {
  return {
    algorithm: 'kmeans',
    cleaningProfile: 'plain',
    languageColumn: '',
    minClusterSize: undefined,
  };
}

function currentDraftConfig(): unknown {
  if (kind.value === 'llm_prompt') {
    return { ...llmDraft.value };
  }
  if (kind.value === 'topic_model') {
    const config: TopicModelConfig = {
      algorithm: topicDraft.value.algorithm || null,
      cleaning_profile: topicDraft.value.cleaningProfile || null,
      embedder: null,
      language_column: topicDraft.value.languageColumn || null,
      min_cluster_size: topicDraft.value.minClusterSize ?? null,
      text_columns: topicTextColumns.value.length > 0 ? topicTextColumns.value : null,
    };
    return config;
  }
  return {};
}

function snapshot(): string {
  return JSON.stringify({ config: currentDraftConfig(), name: draftName.value });
}

const isDirty = computed(() => snapshot() !== savedSnapshot.value);

function loadFromFn(fn: EnrichFunction | null): void {
  saveError.value = null;
  if (fn == null) {
    draftName.value = '';
    llmDraft.value = emptyLlmConfig();
    topicDraft.value = emptyTopicForm();
    topicTextColumns.value = [];
  } else {
    draftName.value = fn.name;
    if (fn.kind === 'llm_prompt') {
      const config = fn.config as Partial<LlmPromptConfig>;
      llmDraft.value = {
        input_columns: config.input_columns ?? [],
        model: config.model ?? null,
        outputs: config.outputs ?? [],
        prompt_template: config.prompt_template ?? '',
        provider_id: config.provider_id ?? 'default',
      };
    } else if (fn.kind === 'topic_model') {
      const config = fn.config as Partial<TopicModelConfig>;
      topicDraft.value = {
        algorithm: config.algorithm ?? 'kmeans',
        cleaningProfile: config.cleaning_profile ?? 'plain',
        languageColumn: config.language_column ?? '',
        minClusterSize: config.min_cluster_size ?? undefined,
      };
      topicTextColumns.value = config.text_columns ?? [];
    }
  }
  savedSnapshot.value = snapshot();
}

watch(
  () => [props.fn?.id, props.fn?.version, props.createKind] as const,
  () => loadFromFn(props.fn),
  { immediate: true },
);

// ---------------------------------------------------------------------------
// Sample loop (llm_prompt only)
// ---------------------------------------------------------------------------

const sample = useSampleRun(() => props.fn?.id ?? null);
const lastTestedConfig = ref('');
const compareOpen = ref(false);

const needsRetest = computed(
  () =>
    sample.runCount.value > 0 && JSON.stringify(currentDraftConfig()) !== lastTestedConfig.value,
);

async function testSample(limit?: number): Promise<void> {
  const config = currentDraftConfig();
  lastTestedConfig.value = JSON.stringify(config);
  await sample.run(config, limit);
}

async function widenSample(): Promise<void> {
  const config = currentDraftConfig();
  lastTestedConfig.value = JSON.stringify(config);
  await sample.widen(config);
}

const outputNames = computed(() => llmDraft.value.outputs.map((o) => o.name).filter(Boolean));
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

// ---------------------------------------------------------------------------
// Save + promoted-edit rerun dialog
// ---------------------------------------------------------------------------

const rerunModalOpen = ref(false);

async function handleSave(): Promise<void> {
  if (isNew.value) {
    await persist('none', true);
    return;
  }
  if (props.fn?.status === 'promoted' && kind.value === 'llm_prompt') {
    rerunModalOpen.value = true;
    return;
  }
  await persist('none', false);
}

async function handleRerunChoice(rerun: 'none' | 'missing' | 'all'): Promise<void> {
  rerunModalOpen.value = false;
  await persist(rerun, false);
}

async function persist(rerun: 'none' | 'missing' | 'all', create: boolean): Promise<void> {
  saving.value = true;
  saveError.value = null;
  try {
    const config = currentDraftConfig();
    let saved: EnrichFunction | null = null;
    if (create) {
      saved = await enrichFnApi.create(props.sourceId, props.table, {
        config,
        kind: kind.value,
        name: draftName.value.trim(),
      });
    } else {
      const id = props.fn?.id;
      if (id == null) {
        return;
      }
      saved = await enrichFnApi.update(id, { config, rerun });
    }
    if (saved == null) {
      throw new Error('Save failed');
    }
    savedSnapshot.value = snapshot();
    emit('saved', saved.id);
    // oxlint-disable-next-line unicorn/catch-error-name -- saveError is the ref
  } catch (err) {
    saveError.value = err instanceof Error ? err.message : 'Save failed';
  } finally {
    saving.value = false;
  }
}

// ---------------------------------------------------------------------------
// Promote / runs / versions / delete
// ---------------------------------------------------------------------------

const promoting = ref(false);

async function togglePromoted(promoted: boolean): Promise<void> {
  const id = props.fn?.id;
  if (id == null) {
    return;
  }
  promoting.value = true;
  try {
    await (promoted ? enrichFnApi.promote(id) : enrichFnApi.demote(id));
    emit('saved', id);
  } finally {
    promoting.value = false;
  }
}

const enrichRun = useEnrichRun(() => {
  if (props.fn != null) {
    emit('saved', props.fn.id);
  }
});
const runModalOpen = ref(false);

onMounted(() => {
  const activeRunId = props.fn?.activeRunId;
  if (activeRunId != null) {
    void enrichRun.resume(activeRunId);
  }
});

async function confirmRun(scope: RunScope): Promise<void> {
  runModalOpen.value = false;
  const id = props.fn?.id;
  if (id != null) {
    await enrichRun.start(id, scope);
  }
}

async function rerunFailed(): Promise<void> {
  const id = props.fn?.id;
  if (id != null) {
    await enrichRun.start(id, 'failed');
  }
}

const historyOpen = ref(false);

function restoreVersion(config: unknown): void {
  if (kind.value === 'llm_prompt') {
    const restored = config as Partial<LlmPromptConfig>;
    llmDraft.value = {
      input_columns: restored.input_columns ?? [],
      model: restored.model ?? null,
      outputs: restored.outputs ?? [],
      prompt_template: restored.prompt_template ?? '',
      provider_id: restored.provider_id ?? 'default',
    };
  }
  historyOpen.value = false;
}

const deleteItems = computed(() => [
  [
    {
      icon: 'i-lucide-trash-2',
      label: 'Delete function (keep columns)',
      onSelect: () => void deleteFn(false),
    },
    {
      color: 'error' as const,
      icon: 'i-lucide-trash-2',
      label: 'Delete function + drop columns',
      onSelect: () => void deleteFn(true),
    },
  ],
]);

async function deleteFn(dropColumns: boolean): Promise<void> {
  const id = props.fn?.id;
  if (id == null) {
    return;
  }
  await enrichFnApi.delete(id, dropColumns);
  emit('deleted');
}
</script>

<template>
  <div class="space-y-4 overflow-y-auto p-4">
    <!-- Header row -->
    <div class="flex items-center gap-3">
      <UInput
        v-model="draftName"
        :disabled="!isNew"
        placeholder="column_name"
        size="md"
        class="w-56 font-mono"
      />
      <span v-if="fn" class="text-sm text-muted">v{{ fn.version }}</span>
      <div class="ml-auto flex items-center gap-2">
        <UButton
          v-if="fn"
          size="md"
          color="neutral"
          variant="ghost"
          icon="i-lucide-history"
          @click="historyOpen = true"
        >
          History
        </UButton>
        <UDropdownMenu v-if="fn" :items="deleteItems">
          <UButton size="md" color="neutral" variant="ghost" icon="i-lucide-trash-2" />
        </UDropdownMenu>
        <UButton
          size="md"
          color="primary"
          :loading="saving"
          :disabled="!isDirty || (isNew && draftName.trim() === '')"
          @click="handleSave"
        >
          {{ isNew ? 'Create draft' : 'Save' }}
        </UButton>
      </div>
    </div>
    <p v-if="saveError" class="text-sm text-error">{{ saveError }}</p>

    <!-- Active / last run -->
    <ActiveRunBar
      v-if="enrichRun.run.value"
      :run="enrichRun.run.value"
      @cancel="enrichRun.cancel()"
      @rerun-failed="rerunFailed"
      @dismiss="enrichRun.clear()"
    />
    <p v-if="enrichRun.error.value" class="text-sm text-error">{{ enrichRun.error.value }}</p>

    <!-- llm_prompt editor -->
    <template v-if="kind === 'llm_prompt'">
      <LlmPromptForm v-model="llmDraft" :columns="columnNames" />

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
              @click="runModalOpen = true"
            >
              Run all
            </UButton>
          </div>
        </div>
        <p v-if="sample.error.value" class="text-sm text-error">{{ sample.error.value }}</p>

        <template v-if="sample.rows.value.length > 0">
          <div class="flex items-center gap-3 text-sm text-muted">
            <span>
              {{ sample.rows.value.length }} rows ·
              {{ sample.result.value?.totalTokens ?? 0 }} tokens ·
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
                    <span
                      v-for="name in outputNames"
                      :key="name"
                      class="block text-muted line-through"
                    >
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

    <!-- topic_model editor -->
    <template v-else-if="kind === 'topic_model'">
      <div class="max-w-md space-y-3">
        <div>
          <p class="mb-1 text-sm font-medium text-highlighted">Text columns</p>
          <USelectMenu
            v-model="topicTextColumns"
            :items="columnNames"
            multiple
            size="md"
            class="w-full"
            placeholder="Columns to embed (first = headline)"
          />
        </div>
        <TopicModelForm v-model="topicDraft" />
        <RouterLink
          v-if="fn"
          :to="{ name: 'topics-table', params: { sourceId, table } }"
          class="inline-flex items-center gap-1 text-sm text-primary-500 hover:underline"
        >
          View clusters
          <UIcon name="i-lucide-arrow-right" class="size-4" />
        </RouterLink>
      </div>
    </template>

    <!-- classifier placeholder -->
    <p v-else class="text-sm text-muted">
      Classifier functions are configured from the Topics tool (taxonomy + curation) — a dedicated
      editor is coming soon.
    </p>

    <!-- Lifecycle -->
    <div v-if="fn" class="border-t border-default pt-4">
      <PromoteToggle :status="fn.status" :loading="promoting" @toggle="togglePromoted" />
    </div>

    <!-- Modals -->
    <RerunPromptModal
      v-model:open="rerunModalOpen"
      :row-count="fn?.staleRowCount ?? null"
      @choose="handleRerunChoice"
    />
    <RunAllModal
      v-if="fn"
      v-model:open="runModalOpen"
      :function-id="fn.id"
      initial-scope="missing"
      @confirm="confirmRun"
    />
    <VersionHistoryPanel
      v-if="fn"
      v-model:open="historyOpen"
      :function-id="fn.id"
      :current-version="fn.version"
      @restore="restoreVersion"
    />
  </div>
</template>

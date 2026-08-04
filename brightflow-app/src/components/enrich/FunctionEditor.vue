<script setup lang="ts">
/**
 * Detail-pane shell of the Enrich tool: owns the draft name + config for one
 * function and its lifecycle — save (with a rerun-choice dialog when editing
 * a promoted prompt), full-table runs, promote/demote, version restore, and
 * delete. The kind-specific editing surfaces live in LlmFunctionEditor and
 * TopicFunctionEditor behind a `v-model:config`. Dirtiness is a JSON
 * snapshot comparison gating Save and Run all.
 */

import { computed, onMounted, ref, useTemplateRef, watch } from 'vue';

import { useEnrichRun } from '@/composables/useEnrichRun';
import { enrichFnApi } from '@/services/api';
import type {
  EnrichFunction,
  FunctionKind,
  LlmPromptConfig,
  RunScope,
  TopicModelConfig,
} from '@/types/enrichment';

import ActiveRunBar from './ActiveRunBar.vue';
import LlmFunctionEditor from './LlmFunctionEditor.vue';
import PromoteToggle from './PromoteToggle.vue';
import RerunPromptModal from './RerunPromptModal.vue';
import RunAllModal from './RunAllModal.vue';
import TopicFunctionEditor from './TopicFunctionEditor.vue';
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

const llmEditor = useTemplateRef('llmEditor');

// ---------------------------------------------------------------------------
// Draft state (the shell owns it; isDirty gates save + Run all)
// ---------------------------------------------------------------------------

const draftName = ref('');
const llmConfig = ref<LlmPromptConfig>(emptyLlmConfig());
const topicConfig = ref<TopicModelConfig>(emptyTopicConfig());
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

function emptyTopicConfig(): TopicModelConfig {
  return {
    algorithm: 'kmeans',
    cleaning_profile: 'plain',
    embedder: null,
    language_column: null,
    min_cluster_size: null,
    text_columns: null,
  };
}

function currentDraftConfig(): unknown {
  if (kind.value === 'llm_prompt') {
    return { ...llmConfig.value };
  }
  if (kind.value === 'topic_model') {
    return { ...topicConfig.value };
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
    llmConfig.value = emptyLlmConfig();
    topicConfig.value = emptyTopicConfig();
  } else {
    draftName.value = fn.name;
    if (fn.kind === 'llm_prompt') {
      const config = fn.config as Partial<LlmPromptConfig>;
      llmConfig.value = {
        input_columns: config.input_columns ?? [],
        model: config.model ?? null,
        outputs: config.outputs ?? [],
        prompt_template: config.prompt_template ?? '',
        provider_id: config.provider_id ?? 'default',
      };
    } else if (fn.kind === 'topic_model') {
      const config = fn.config as Partial<TopicModelConfig>;
      topicConfig.value = {
        algorithm: config.algorithm ?? 'kmeans',
        cleaning_profile: config.cleaning_profile ?? 'plain',
        /* Deliberately not restored: only one embedder exists today. */
        embedder: null,
        language_column: config.language_column ?? null,
        min_cluster_size: config.min_cluster_size ?? null,
        text_columns:
          config.text_columns != null && config.text_columns.length > 0
            ? config.text_columns
            : null,
      };
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
  } catch (error) {
    saveError.value = error instanceof Error ? error.message : 'Save failed';
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
    llmEditor.value?.restoreConfig(config as Partial<LlmPromptConfig>);
  }
  historyOpen.value = false;
}

const deleteItems = computed(() => [
  [
    {
      icon: 'i-lucide-trash-2',
      label: 'Delete function (keep columns)',
      onSelect: (): void => void deleteFn(false),
    },
    {
      color: 'error' as const,
      icon: 'i-lucide-trash-2',
      label: 'Delete function + drop columns',
      onSelect: (): void => void deleteFn(true),
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
    <p v-if="enrichRun.errorMessage.value" class="text-sm text-error">
      {{ enrichRun.errorMessage.value }}
    </p>

    <!-- Kind-specific editor -->
    <LlmFunctionEditor
      v-if="kind === 'llm_prompt'"
      ref="llmEditor"
      v-model:config="llmConfig"
      :column-names="columnNames"
      :fn="fn"
      :is-dirty="isDirty"
      @run-all="runModalOpen = true"
    />
    <TopicFunctionEditor
      v-else-if="kind === 'topic_model'"
      v-model:config="topicConfig"
      :column-names="columnNames"
      :has-fn="fn != null"
      :source-id="sourceId"
      :table="table"
    />

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

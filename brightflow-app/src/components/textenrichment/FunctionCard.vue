<script setup lang="ts">
/**
 * One text function (classify or extract), self-contained: it finds its own
 * function row by kind, creates it when absent, and otherwise edits the
 * saved config, tests on a sample, runs with an explicit scope, restores a
 * version, deletes. The card owns the expand state: once a function exists
 * it collapses to a one-line header — name, version, columns, provider, rows
 * computed of total, rows to recompute — with Run and Edit; the editor only
 * shows while editing or creating. A saved edit asks for the rerun scope,
 * because a config change empties the cache.
 *
 * Queries share keys with the rest of the tool (`enrich-fns`,
 * `vocabulary-health`, `tables-index`, `llm-providers`) so one fetch serves
 * every card and pane on the page.
 */

import { useQuery, useQueryCache } from '@pinia/colada';
import { computed, onMounted, ref, watch } from 'vue';

import { useEnrichRun } from '@/composables/useEnrichRun';
import { enrichFnApi, llmApi, tableApi, vocabularyApi } from '@/services/api';
import type {
  EnrichFunction,
  FunctionKind,
  RunScope,
  TicketFunctionConfig,
} from '@/types/enrichment';

import ActiveRunBar from './ActiveRunBar.vue';
import RunAllModal from './RunAllModal.vue';
import TicketFunctionEditor from './TicketFunctionEditor.vue';
import VersionHistoryPanel from './VersionHistoryPanel.vue';

const props = defineProps<{
  kind: FunctionKind;
  sourceId: string;
  table: string;
}>();

const queryCache = useQueryCache();

const functionsKey = computed(() => ['enrich-fns', props.sourceId, props.table]);
const { data: functions } = useQuery({
  key: () => functionsKey.value,
  query: async () => (await enrichFnApi.list(props.sourceId, props.table)) ?? [],
});
const fn = computed<EnrichFunction | null>(
  () => (functions.value ?? []).find((f) => f.kind === props.kind) ?? null,
);

const { data: tableIndex } = useQuery({
  key: () => ['tables-index', props.sourceId],
  query: async () => (await tableApi.listAvailable()) ?? [],
});
const columnNames = computed(() => {
  const info = (tableIndex.value ?? []).find(
    (t) => t.source_id === props.sourceId && t.name === props.table,
  );
  const schema = info?.schema as {
    columns?: { name?: string }[];
    fields?: { name?: string }[];
  } | null;
  return (schema?.columns ?? schema?.fields ?? [])
    .map((f) => f.name)
    .filter((n): n is string => typeof n === 'string');
});

const { data: health } = useQuery({
  key: () => ['vocabulary-health', props.sourceId, props.table],
  query: () => vocabularyApi.health(props.sourceId, props.table),
});
const totalRows = computed(() => health.value?.totalRows ?? null);

const { data: providers } = useQuery({
  key: ['llm-providers'],
  query: async () => (await llmApi.listProviders()) ?? [],
});

const title = computed(() =>
  props.kind === 'ticket_classify' ? 'Summary and classification' : 'Extraction',
);
const defaultName = computed(() => (props.kind === 'ticket_classify' ? 'classify' : 'extract'));

function emptyConfig(): TicketFunctionConfig {
  return { language_column: null, model: null, provider_id: 'default', text_columns: [] };
}

const config = ref<TicketFunctionConfig>(emptyConfig());
const savedSnapshot = ref('');
const saving = ref(false);
const saveError = ref<string | null>(null);
const creating = ref(false);
const editing = ref(false);

function snapshot(): string {
  return JSON.stringify(config.value);
}

const isDirty = computed(() => snapshot() !== savedSnapshot.value);

function loadFromFn(current: EnrichFunction | null): void {
  saveError.value = null;
  if (current == null) {
    config.value = emptyConfig();
  } else {
    const c = current.config as Partial<TicketFunctionConfig>;
    config.value = {
      language_column: c.language_column ?? null,
      model: c.model ?? null,
      provider_id: c.provider_id ?? 'default',
      text_columns: c.text_columns ?? [],
    };
  }
  savedSnapshot.value = snapshot();
}

watch(
  () => [fn.value?.id, fn.value?.version] as const,
  () => loadFromFn(fn.value),
  {
    immediate: true,
  },
);

/** The header's one-line summary of the saved config. */
const providerLabel = computed(() => {
  const saved = fn.value?.config as Partial<TicketFunctionConfig> | undefined;
  const id = saved?.provider_id ?? 'default';
  const hit = (providers.value ?? []).find((p) => String(p.id) === id);
  let base = hit?.name ?? id;
  if (hit == null && id === 'default') {
    base = 'default provider';
  }
  return saved?.model == null ? base : `${base} · ${saved.model}`;
});
const columnsLabel = computed(() => {
  const saved = fn.value?.config as Partial<TicketFunctionConfig> | undefined;
  return (saved?.text_columns ?? []).join(', ');
});
const computedRows = computed(() => {
  if (totalRows.value == null) {
    return null;
  }
  return Math.max(0, totalRows.value - (fn.value?.staleRowCount ?? 0));
});

async function refresh(): Promise<void> {
  await queryCache.invalidateQueries({ key: functionsKey.value });
}

async function persist(rerun: 'none' | 'missing' | 'all'): Promise<void> {
  saving.value = true;
  saveError.value = null;
  try {
    const saved =
      fn.value == null
        ? await enrichFnApi.create(props.sourceId, props.table, {
            config: { ...config.value },
            kind: props.kind,
            name: defaultName.value,
          })
        : await enrichFnApi.update(fn.value.id, { config: { ...config.value }, rerun });
    if (saved == null) {
      throw new Error('Save failed');
    }
    savedSnapshot.value = snapshot();
    creating.value = false;
    editing.value = false;
    await refresh();
  } catch (error) {
    saveError.value = error instanceof Error ? error.message : 'Save failed';
  } finally {
    saving.value = false;
  }
}

const rerunChoiceOpen = ref(false);

async function handleSave(): Promise<void> {
  if (fn.value == null) {
    await persist('none');
    return;
  }
  rerunChoiceOpen.value = true;
}

async function chooseRerun(rerun: 'none' | 'missing' | 'all'): Promise<void> {
  rerunChoiceOpen.value = false;
  await persist(rerun);
}

function cancelEdit(): void {
  loadFromFn(fn.value);
  editing.value = false;
  creating.value = false;
}

const enrichRun = useEnrichRun(() => void refresh());
const runModalOpen = ref(false);

onMounted(() => {
  const activeRunId = fn.value?.activeRunId;
  if (activeRunId != null) {
    void enrichRun.resume(activeRunId);
  }
});

async function confirmRun(scope: RunScope): Promise<void> {
  runModalOpen.value = false;
  if (fn.value != null) {
    await enrichRun.start(fn.value.id, scope);
  }
}

async function rerunFailed(): Promise<void> {
  if (fn.value != null) {
    await enrichRun.start(fn.value.id, 'failed');
  }
}

const historyOpen = ref(false);

function restoreVersion(restored: unknown): void {
  const c = restored as Partial<TicketFunctionConfig>;
  config.value = {
    language_column: c.language_column ?? null,
    model: c.model ?? null,
    provider_id: c.provider_id ?? 'default',
    text_columns: c.text_columns ?? [],
  };
  historyOpen.value = false;
  editing.value = true;
}

async function remove(): Promise<void> {
  if (fn.value == null) {
    return;
  }
  const dropColumns = window.confirm(
    `Delete "${fn.value.name}"?\n\nOK also drops its materialised columns; Cancel keeps the function.`,
  );
  if (!dropColumns) {
    return;
  }
  await enrichFnApi.delete(fn.value.id, true);
  editing.value = false;
  await refresh();
}

const expanded = computed(() => creating.value || editing.value);
</script>

<template>
  <div class="rounded-lg border border-default bg-elevated p-4">
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
      <h4 class="text-sm font-medium text-highlighted">{{ title }}</h4>
      <template v-if="fn">
        <span class="text-sm text-muted">{{ fn.name }} · v{{ fn.version }}</span>
        <span v-if="columnsLabel" class="text-sm text-muted">· {{ columnsLabel }}</span>
        <span class="text-sm text-muted">· {{ providerLabel }}</span>
        <span v-if="computedRows != null" class="text-sm text-muted">
          · {{ computedRows.toLocaleString() }} of {{ totalRows?.toLocaleString() }} rows
        </span>
        <span v-if="(fn.staleRowCount ?? 0) > 0" class="text-sm text-warning">
          · {{ fn.staleRowCount?.toLocaleString() }} rows to recompute
        </span>
      </template>
      <div class="ml-auto flex items-center gap-2">
        <template v-if="fn && !expanded">
          <UButton
            size="md"
            color="primary"
            variant="soft"
            icon="i-lucide-play"
            @click="runModalOpen = true"
          >
            Run
          </UButton>
          <UButton
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-pencil"
            @click="editing = true"
          >
            Edit
          </UButton>
        </template>
        <template v-else-if="fn">
          <UButton
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-history"
            @click="historyOpen = true"
          >
            History
          </UButton>
          <UButton
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-trash-2"
            aria-label="Delete function"
            @click="() => void remove()"
          />
          <UButton size="md" color="neutral" variant="ghost" @click="cancelEdit">Close</UButton>
          <UButton
            size="md"
            color="primary"
            :loading="saving"
            :disabled="!isDirty"
            @click="() => void handleSave()"
          >
            Save
          </UButton>
        </template>
        <UButton
          v-else-if="!creating"
          size="md"
          color="primary"
          variant="soft"
          icon="i-lucide-plus"
          @click="creating = true"
        >
          Set up
        </UButton>
        <template v-else>
          <UButton size="md" color="neutral" variant="ghost" @click="cancelEdit">Cancel</UButton>
          <UButton
            size="md"
            color="primary"
            :loading="saving"
            :disabled="config.text_columns.length === 0"
            @click="() => void handleSave()"
          >
            Create
          </UButton>
        </template>
      </div>
    </div>
    <p v-if="saveError" class="mt-2 text-sm text-error">{{ saveError }}</p>

    <div v-if="fn != null || creating" class="mt-4 space-y-4">
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

      <TicketFunctionEditor
        v-if="expanded"
        v-model:config="config"
        :column-names="columnNames"
        :fn="fn"
        :kind="kind"
        :source-id="sourceId"
        :table="table"
        @run-all="runModalOpen = true"
      />
    </div>
    <p v-else class="mt-2 text-sm text-muted">
      Not set up yet — every row in this table stays unclassified until it is.
    </p>

    <UModal v-model:open="rerunChoiceOpen" title="Save changes">
      <template #body>
        <p class="text-sm text-muted">
          A changed configuration starts an empty cache. What should recompute now?
        </p>
        <div class="mt-4 flex flex-wrap gap-2">
          <UButton size="md" color="neutral" variant="soft" @click="() => void chooseRerun('none')">
            Save only
          </UButton>
          <UButton
            size="md"
            color="neutral"
            variant="soft"
            @click="() => void chooseRerun('missing')"
          >
            Save + run missing rows
          </UButton>
          <UButton size="md" color="primary" variant="soft" @click="() => void chooseRerun('all')">
            Save + recompute everything
          </UButton>
        </div>
      </template>
    </UModal>
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

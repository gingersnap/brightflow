<script setup lang="ts">
/**
 * One ticket function (classify or extract): create when absent, otherwise
 * edit the saved config, test on a sample, run with an explicit scope,
 * restore a version, delete. Owns the draft config; TicketFunctionEditor is
 * the form. A saved edit to a running function asks for the rerun scope the
 * same way the old editor did, because a config change empties the cache.
 */

import { computed, onMounted, ref, watch } from 'vue';

import { useEnrichRun } from '@/composables/useEnrichRun';
import { enrichFnApi } from '@/services/api';
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
  fn: EnrichFunction | null;
  columnNames: string[];
  sourceId: string;
  table: string;
}>();

const emit = defineEmits<{
  changed: [];
}>();

const title = computed(() =>
  props.kind === 'ticket_classify' ? 'Classification' : 'Mention extraction',
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

function snapshot(): string {
  return JSON.stringify(config.value);
}

const isDirty = computed(() => snapshot() !== savedSnapshot.value);

function loadFromFn(fn: EnrichFunction | null): void {
  saveError.value = null;
  if (fn == null) {
    config.value = emptyConfig();
  } else {
    const c = fn.config as Partial<TicketFunctionConfig>;
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
  () => [props.fn?.id, props.fn?.version] as const,
  () => loadFromFn(props.fn),
  {
    immediate: true,
  },
);

async function persist(rerun: 'none' | 'missing' | 'all'): Promise<void> {
  saving.value = true;
  saveError.value = null;
  try {
    const saved =
      props.fn == null
        ? await enrichFnApi.create(props.sourceId, props.table, {
            config: { ...config.value },
            kind: props.kind,
            name: defaultName.value,
          })
        : await enrichFnApi.update(props.fn.id, { config: { ...config.value }, rerun });
    if (saved == null) {
      throw new Error('Save failed');
    }
    savedSnapshot.value = snapshot();
    creating.value = false;
    emit('changed');
  } catch (error) {
    saveError.value = error instanceof Error ? error.message : 'Save failed';
  } finally {
    saving.value = false;
  }
}

const rerunChoiceOpen = ref(false);

async function handleSave(): Promise<void> {
  if (props.fn == null) {
    await persist('none');
    return;
  }
  rerunChoiceOpen.value = true;
}

async function chooseRerun(rerun: 'none' | 'missing' | 'all'): Promise<void> {
  rerunChoiceOpen.value = false;
  await persist(rerun);
}

const enrichRun = useEnrichRun(() => emit('changed'));
const runModalOpen = ref(false);

onMounted(() => {
  const activeRunId = props.fn?.activeRunId;
  if (activeRunId != null) {
    void enrichRun.resume(activeRunId);
  }
});

async function confirmRun(scope: RunScope): Promise<void> {
  runModalOpen.value = false;
  if (props.fn != null) {
    await enrichRun.start(props.fn.id, scope);
  }
}

async function rerunFailed(): Promise<void> {
  if (props.fn != null) {
    await enrichRun.start(props.fn.id, 'failed');
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
}

async function remove(): Promise<void> {
  if (props.fn == null) {
    return;
  }
  const dropColumns = window.confirm(
    `Delete "${props.fn.name}"?\n\nOK also drops its materialised columns; Cancel keeps the function.`,
  );
  if (!dropColumns) {
    return;
  }
  await enrichFnApi.delete(props.fn.id, true);
  emit('changed');
}
</script>

<template>
  <div class="rounded-lg border border-default bg-elevated p-4">
    <div class="flex items-center gap-3">
      <h4 class="text-sm font-medium text-highlighted">{{ title }}</h4>
      <span v-if="fn" class="text-sm text-muted">{{ fn.name }} · v{{ fn.version }}</span>
      <div class="ml-auto flex items-center gap-2">
        <template v-if="fn">
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
          <UButton size="md" color="neutral" variant="ghost" @click="creating = false">
            Cancel
          </UButton>
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
      Not set up yet — every ticket in this table stays unclassified until it is.
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

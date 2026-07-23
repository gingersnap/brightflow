<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

import { agentApi, llmApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';
import type { AgentRunEventPayload, AgentRunResponse } from '@/types/generated';

/**
 * Agent-run trigger buttons. Hidden entirely unless an LLM provider is
 * configured. Proposals land in the activity feed for approve/reject.
 *
 * Run progress arrives as pushed `agentRun` WS events — no polling.
 */
const props = defineProps<{
  sourceId: string;
  table: string;
  /** Which run kinds to offer, e.g. ['auto_label','propose_merges'] */
  kinds: { kind: string; label: string; icon: string }[];
}>();

/** Structural guard for a pushed `agentRun` frame. */
function isAgentRunEvent(
  m: Record<string, unknown>,
): m is Record<string, unknown> & AgentRunEventPayload {
  return typeof m['run'] === 'object' && m['run'] != null;
}

const connection = useConnectionStore();
const hasProvider = ref(false);
const activeRun = ref<AgentRunResponse | null>(null);
/** Last finished run — the anchor for "Undo all" after an auto-apply run. */
const lastRun = ref<AgentRunResponse | null>(null);
const lastResult = ref<string | null>(null);
/**
 * Auto-apply by default: every tool the runner hands out is undoable, so
 * reversibility (not pre-approval) is the safety mechanism.
 */
const autoApply = ref(true);
const undoingAll = ref(false);
let unsubscribe: (() => void) | null = null;

onMounted(async () => {
  unsubscribe = connection.onMessage('agentRun', (message) => {
    if (!isAgentRunEvent(message)) {
      return;
    }
    const { run } = message;
    if (activeRun.value == null || activeRun.value.id !== run.id) {
      return;
    }
    if (run.status === 'running') {
      activeRun.value = run;
      return;
    }
    // Completed runs show only the stats head here; the full prose (after
    // The first newline) renders in the insights NarrationPanel.
    lastResult.value =
      run.status === 'completed'
        ? (run.detail?.split('\n')[0] ?? 'Done — see Activity')
        : `${run.status}: ${run.detail ?? ''}`;
    lastRun.value = run;
    activeRun.value = null;
  });
  const providers = await llmApi.listProviders();
  hasProvider.value = (providers?.length ?? 0) > 0;
});

onBeforeUnmount(() => {
  unsubscribe?.();
});

const canUndoAll = computed(
  () =>
    lastRun.value != null &&
    lastRun.value.status === 'completed' &&
    lastRun.value.mode === 'auto_apply',
);

async function start(kind: string): Promise<void> {
  lastResult.value = null;
  lastRun.value = null;
  const run = await agentApi.start({
    kind,
    sourceId: props.sourceId,
    table: props.table,
    mode: autoApply.value ? 'auto_apply' : 'propose',
  });
  if (run == null) {
    lastResult.value = 'Failed to start agent run';
    return;
  }
  activeRun.value = run;
}

async function cancel(): Promise<void> {
  if (activeRun.value != null) {
    await agentApi.cancel(activeRun.value.id);
  }
}

/** Revert everything the last auto-apply run did, newest first. */
async function undoAll(): Promise<void> {
  const run = lastRun.value;
  if (run == null) {
    return;
  }
  const confirmed = window.confirm(
    `Undo everything agent run #${run.id} applied?\n\n` +
      `Actions are reverted newest first. Anything that cannot be reverted ` +
      `(e.g. a category whose labels changed since) is reported and skipped.`,
  );
  if (!confirmed) {
    return;
  }
  undoingAll.value = true;
  try {
    const result = await agentApi.undoAll(run.id);
    if (result == null) {
      lastResult.value = 'Undo all failed';
      return;
    }
    lastResult.value =
      result.failed > 0
        ? `Undid ${result.undone} of ${result.total}; ${result.failed} failed — see Activity`
        : `Undid ${result.undone} action${result.undone === 1 ? '' : 's'}`;
    lastRun.value = null;
  } finally {
    undoingAll.value = false;
  }
}
</script>

<template>
  <div v-if="hasProvider" class="flex flex-wrap items-center gap-2">
    <template v-if="activeRun == null">
      <UButton
        v-for="entry in kinds"
        :key="entry.kind"
        size="md"
        color="neutral"
        variant="soft"
        :icon="entry.icon"
        @click="start(entry.kind)"
      >
        {{ entry.label }}
      </UButton>
      <USwitch v-model="autoApply" label="Auto-apply (undoable)" />
      <UButton
        v-if="canUndoAll"
        size="md"
        color="neutral"
        variant="soft"
        icon="i-lucide-rotate-ccw"
        :loading="undoingAll"
        @click="() => void undoAll()"
      >
        Undo all
      </UButton>
    </template>
    <template v-else>
      <span class="text-sm text-muted">
        Agent running ({{ activeRun.kind.replaceAll('_', ' ') }})…
      </span>
      <UButton size="xs" color="neutral" variant="ghost" @click="cancel">Cancel</UButton>
    </template>
    <span v-if="lastResult" class="max-w-md truncate text-sm text-muted" :title="lastResult">
      {{ lastResult }}
    </span>
  </div>
</template>

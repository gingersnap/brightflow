<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';

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
const lastResult = ref<string | null>(null);
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
    lastResult.value =
      run.status === 'completed'
        ? (run.detail ?? 'Done — review proposals in Activity')
        : `${run.status}: ${run.detail ?? ''}`;
    activeRun.value = null;
  });
  const providers = await llmApi.listProviders();
  hasProvider.value = (providers?.length ?? 0) > 0;
});

onBeforeUnmount(() => {
  unsubscribe?.();
});

async function start(kind: string): Promise<void> {
  lastResult.value = null;
  const run = await agentApi.start(kind, props.sourceId, props.table);
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

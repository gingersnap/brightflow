<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';

import { agentApi, llmApi } from '@/services/api';
import { useCurationStore } from '@/stores/curation';
import type { AgentRunResponse } from '@/types/generated';

/**
 * Agent-run trigger buttons. Hidden entirely unless an LLM provider is
 * configured. Proposals land in the activity feed for approve/reject.
 */
const props = defineProps<{
  sourceId: string;
  table: string;
  /** Which run kinds to offer, e.g. ['auto_label','propose_merges'] */
  kinds: { kind: string; label: string; icon: string }[];
}>();

const curation = useCurationStore();
const hasProvider = ref(false);
const activeRun = ref<AgentRunResponse | null>(null);
const lastResult = ref<string | null>(null);
let pollTimer: ReturnType<typeof setInterval> | null = null;

onMounted(async () => {
  const providers = await llmApi.listProviders();
  hasProvider.value = (providers?.length ?? 0) > 0;
});

onBeforeUnmount(() => {
  if (pollTimer != null) {
    clearInterval(pollTimer);
  }
});

async function start(kind: string): Promise<void> {
  lastResult.value = null;
  const run = await agentApi.start(kind, props.sourceId, props.table);
  if (run == null) {
    lastResult.value = 'Failed to start agent run';
    return;
  }
  activeRun.value = run;
  pollTimer = setInterval(() => void poll(), 2000);
}

async function poll(): Promise<void> {
  if (activeRun.value == null) {
    return;
  }
  const run = await agentApi.get(activeRun.value.id);
  if (run == null) {
    return;
  }
  activeRun.value = run;
  if (run.status !== 'running') {
    if (pollTimer != null) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
    lastResult.value =
      run.status === 'completed'
        ? (run.detail ?? 'Done — review proposals in Activity')
        : `${run.status}: ${run.detail ?? ''}`;
    activeRun.value = null;
    await curation.refreshFeed();
  }
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

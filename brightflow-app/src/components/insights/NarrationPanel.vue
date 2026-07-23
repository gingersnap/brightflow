<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

import CollapsibleSection from '@/components/common/CollapsibleSection.vue';
import { agentApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';
import type { AgentRunEventPayload, AgentRunResponse } from '@/types/generated';

/**
 * Display surface for LLM narration of the insights: the newest completed
 * narrate/triage run for this table. Agent runs store their output as
 * `detail = "{stats line}\n{narration}"`; the stats head stays in the trigger
 * row (AgentActions), the full prose lives here — and survives reloads via
 * the agent-run list.
 */
const props = defineProps<{
  sourceId: string;
  table: string;
}>();

const NARRATION_KINDS = ['narrate_insights', 'triage_insights'] as const;

function isAgentRunEvent(
  m: Record<string, unknown>,
): m is Record<string, unknown> & AgentRunEventPayload {
  return typeof m['run'] === 'object' && m['run'] != null;
}

const connection = useConnectionStore();
const run = ref<AgentRunResponse | null>(null);
let unsubscribe: (() => void) | null = null;

// Scope format matches the agent runner: `{kind}:{sourceId}:{table}`.
const scopes = computed(
  () => new Set(NARRATION_KINDS.map((kind) => `${kind}:${props.sourceId}:${props.table}`)),
);

function consider(candidate: AgentRunResponse): void {
  if (candidate.status !== 'completed' || !scopes.value.has(candidate.scope)) {
    return;
  }
  if (candidate.detail == null || candidate.detail === '') {
    return;
  }
  if (run.value != null && run.value.createdAt > candidate.createdAt) {
    return;
  }
  run.value = candidate;
}

onMounted(async () => {
  unsubscribe = connection.onMessage('agentRun', (message) => {
    if (isAgentRunEvent(message)) {
      consider(message.run);
    }
  });
  const runs = await agentApi.list(25);
  for (const candidate of runs ?? []) {
    consider(candidate);
  }
});

onBeforeUnmount(() => {
  unsubscribe?.();
});

/** Detail = "{stats line}\n{narration}" — split on the first newline. */
const parsed = computed(() => {
  const detail = run.value?.detail ?? '';
  const newline = detail.indexOf('\n');
  if (newline === -1) {
    return { prose: detail, stats: '' };
  }
  return { prose: detail.slice(newline + 1).trim(), stats: detail.slice(0, newline).trim() };
});

const kindLabel = computed(() => (run.value?.kind === 'triage_insights' ? 'Triage' : 'Summary'));

const finishedAtText = computed(() => {
  const finished = run.value?.finishedAt;
  if (finished == null) {
    return '';
  }
  return new Date(Number(finished) * 1000).toLocaleString();
});
</script>

<template>
  <div v-if="run != null && parsed.prose !== ''" class="rounded-lg border border-default">
    <CollapsibleSection title="Agent summary">
      <template #actions>
        <span class="text-sm text-muted">
          {{ kindLabel }}<template v-if="finishedAtText"> · {{ finishedAtText }}</template>
          <template v-if="parsed.stats"> · {{ parsed.stats }}</template>
        </span>
      </template>
      <p class="px-4 py-3 text-base leading-relaxed whitespace-pre-wrap text-default">
        {{ parsed.prose }}
      </p>
    </CollapsibleSection>
  </div>
</template>

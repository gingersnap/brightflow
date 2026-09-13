<script setup lang="ts">
/**
 * One agent run in the activity feed: what kind of run on which table, its
 * stats line and the model's closing note in full (for a describe run, what
 * it overruled and why), approve-all and reject-all for the proposals still
 * pending, a link to the table's Semantics page, and the run's entries
 * inside, collapsed until opened with their count on the toggle. The
 * entries are the same rows the feed shows on their own, so a person can
 * reject two and approve the rest without leaving the card.
 */

import { computed, ref } from 'vue';

import ActionRow from '@/components/actions/ActionRow.vue';
import { useCuration } from '@/composables/useCuration';
import type { ActionLogEntry, AgentRunResponse } from '@/types/generated';

import { kindLabel, pendingIn, scopeOf, splitDetail } from './activityGroups';

const props = defineProps<{
  runId: number;
  /** Null when the runs list did not carry this run (older than its window). */
  run: AgentRunResponse | null;
  entries: ActionLogEntry[];
}>();

const curation = useCuration();
/** Collapsed until asked: the header carries the count, the note and the controls. */
const expanded = ref(false);
const busy = ref(false);

const pending = computed(() => pendingIn(props.entries));
const scope = computed(() => (props.run == null ? null : scopeOf(props.run)));
const detail = computed(() => splitDetail(props.run?.detail));
const title = computed(() => {
  const kind = props.run == null ? `Run #${props.runId}` : kindLabel(props.run.kind);
  return scope.value == null ? kind : `${kind} · ${scope.value.table}`;
});

function formatTime(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

/**
 * Approve this run's pending proposals. Confirms first, for the same reason
 * the global sweep does: they apply oldest first and there is no bulk undo
 * of approvals as a unit, only per action or "Undo all" on the run.
 */
async function approveRun(): Promise<void> {
  const n = pending.value;
  if (n === 0) {
    return;
  }
  const confirmed = window.confirm(
    `Approve all ${n} pending proposal${n === 1 ? '' : 's'} of this run?\n\n` +
      `They are applied oldest first, in the order they were proposed.`,
  );
  if (!confirmed) {
    return;
  }
  busy.value = true;
  try {
    await curation.approveRun(props.runId);
  } finally {
    busy.value = false;
  }
}

async function rejectRun(): Promise<void> {
  const n = pending.value;
  if (n === 0) {
    return;
  }
  const confirmed = window.confirm(
    `Reject all ${n} pending proposal${n === 1 ? '' : 's'} of this run? Nothing is applied.`,
  );
  if (!confirmed) {
    return;
  }
  busy.value = true;
  try {
    await curation.rejectRun(props.runId);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="px-4 py-3">
    <div class="rounded-lg border border-violet-500/30 bg-violet-500/5">
      <div class="flex flex-wrap items-start justify-between gap-3 px-4 py-3">
        <div class="min-w-0">
          <p class="flex items-center gap-2 text-sm font-medium text-highlighted">
            <UIcon name="i-lucide-bot" class="h-4 w-4 flex-shrink-0 text-violet-500" />
            {{ title }}
            <span class="font-normal text-muted">#{{ runId }}</span>
          </p>
          <p class="mt-0.5 text-xs text-muted">
            <template v-if="run">
              <span
                :class="{
                  'text-amber-500': run.status === 'running',
                  'text-red-500': run.status === 'failed',
                }"
              >
                {{ run.status }}
              </span>
              · {{ run.mode === 'auto_apply' ? 'auto-applied' : 'proposed' }} ·
              {{ formatTime(run.createdAt) }}
              <template v-if="detail.stats"> · {{ detail.stats }}</template>
            </template>
            <template v-else>run details not loaded</template>
            <template v-if="pending > 0"> · {{ pending }} awaiting review</template>
          </p>
        </div>
        <div class="flex flex-shrink-0 flex-wrap items-center gap-2">
          <UButton
            v-if="pending > 0"
            size="md"
            color="primary"
            variant="soft"
            icon="i-lucide-check-check"
            :loading="busy"
            @click="() => void approveRun()"
          >
            Approve all ({{ pending }})
          </UButton>
          <UButton
            v-if="pending > 0"
            size="md"
            color="neutral"
            variant="soft"
            icon="i-lucide-x"
            :loading="busy"
            @click="() => void rejectRun()"
          >
            Reject all
          </UButton>
          <UButton
            v-if="scope"
            size="md"
            color="neutral"
            variant="ghost"
            icon="i-lucide-book-open-text"
            :to="{
              name: 'semantics-table',
              params: { sourceId: scope.sourceId, table: scope.table },
            }"
          >
            Semantics
          </UButton>
          <UButton
            size="md"
            color="neutral"
            variant="ghost"
            :icon="expanded ? 'i-lucide-chevron-up' : 'i-lucide-chevron-down'"
            :aria-expanded="expanded"
            @click="expanded = !expanded"
          >
            {{ entries.length }} action{{ entries.length === 1 ? '' : 's' }}
          </UButton>
        </div>
      </div>
      <p
        v-if="detail.note"
        class="border-t border-violet-500/20 px-4 py-3 text-base whitespace-pre-line text-default"
      >
        {{ detail.note }}
      </p>
      <div v-if="expanded" class="divide-y divide-default border-t border-violet-500/20">
        <ActionRow v-for="entry in entries" :key="entry.id" :entry="entry" />
      </div>
    </div>
  </div>
</template>

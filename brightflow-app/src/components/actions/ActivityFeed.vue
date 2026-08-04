<script setup lang="ts">
/**
 * Curation audit trail: every action — human or agent — in one feed, with
 * per-entry approve/reject and an undo control only where the entry says the
 * action is genuinely undoable. The bulk "Accept all" is the one guarded path;
 * see approveAll for what it confirms and deliberately does not promise.
 */

import { computed } from 'vue';

import { useCuration } from '@/composables/useCuration';
import type { ActionLogEntry } from '@/types/generated';

// Queries fire on setup; the store merges WS pushes on top.
const curation = useCuration();

const entries = computed(() => curation.store.feed);
const pending = computed(() => curation.store.pendingCount);

/**
 * Bulk approve. Confirms first, because this applies every pending proposal at
 * once and there is no matching "undo all" — reversing it means the same
 * one-by-one grind this button exists to avoid.
 *
 * The count comes from the server, not from the visible feed (capped at 100), so
 * the dialog promises exactly what happens. The wording deliberately does NOT
 * claim the batch is undoable: some action kinds aren't, and even
 * `define_taxonomy_category` refuses to undo once rows carry its label. The feed
 * shows an undo control per action where one genuinely exists.
 */
async function approveAll(): Promise<void> {
  const n = pending.value;
  if (n === 0) {
    return;
  }
  const confirmed = window.confirm(
    `Approve all ${n} pending proposal${n === 1 ? '' : 's'}?\n\n` +
      `They are applied oldest first, in the order they were proposed. ` +
      `There is no bulk undo — reversing this means undoing them individually.`,
  );
  if (!confirmed) {
    return;
  }
  const result = await curation.approveAll();
  if (result != null && result.failed > 0) {
    window.alert(
      `Applied ${result.approved} of ${result.total}. ${result.failed} failed — ` +
        `see the feed for details.`,
    );
  }
}

function formatTime(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString();
}

function describe(entry: ActionLogEntry): string {
  const params = entry.params as Record<string, unknown> | null;
  const kind = entry.actionKind.replaceAll('_', ' ');
  if (params == null) {
    return kind;
  }
  const details: string[] = [];
  for (const key of ['name', 'term', 'label', 'column', 'target', 'fingerprint']) {
    const value = params[key];
    if (typeof value === 'string' && value.length > 0) {
      details.push(`${key}=${value.length > 24 ? `${value.slice(0, 24)}…` : value}`);
    }
  }
  for (const key of ['clusterId', 'cluster_id', 'from_cluster_id', 'into_cluster_id']) {
    const value = params[key];
    if (typeof value === 'number') {
      details.push(`#${value}`);
    }
  }
  return details.length > 0 ? `${kind} · ${details.join(' ')}` : kind;
}

const statusColor: Record<string, string> = {
  applied: 'text-emerald-500',
  proposed: 'text-amber-500',
  rejected: 'text-muted',
  undone: 'text-muted line-through',
  failed: 'text-red-500',
};
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="flex items-start justify-between gap-3 border-b border-default px-4 py-3">
      <div class="min-w-0">
        <h2 class="text-sm font-semibold text-highlighted">Activity</h2>
        <p class="text-sm text-muted">Every curation action — by you or an agent.</p>
      </div>
      <UButton
        v-if="pending > 0"
        size="md"
        color="primary"
        variant="soft"
        icon="i-lucide-check-check"
        class="flex-shrink-0"
        :loading="curation.approvingAll.value"
        @click="() => void approveAll()"
      >
        Accept all ({{ pending }})
      </UButton>
    </div>

    <p
      v-if="curation.store.lastError"
      class="border-b border-default px-4 py-2 text-sm text-red-500"
    >
      {{ curation.store.lastError }}
    </p>
    <div class="flex-1 overflow-y-auto">
      <p v-if="entries.length === 0" class="p-4 text-sm text-muted">Loading…</p>
      <p v-else-if="entries.length === 0" class="p-4 text-sm text-muted">No actions yet.</p>
      <ul v-else class="divide-y divide-default">
        <li v-for="entry in entries" :key="entry.id" class="flex items-start gap-3 px-4 py-3">
          <UIcon
            :name="entry.actorType === 'agent' ? 'i-lucide-bot' : 'i-lucide-user'"
            class="mt-0.5 h-4 w-4 flex-shrink-0"
            :class="entry.actorType === 'agent' ? 'text-violet-500' : 'text-muted'"
          />
          <div class="min-w-0 flex-1">
            <p class="text-sm text-highlighted">{{ describe(entry) }}</p>
            <p class="text-xs text-muted">
              <span :class="statusColor[entry.status] ?? 'text-muted'">{{ entry.status }}</span>
              · {{ formatTime(entry.createdAt) }}
              <template v-if="entry.agentRunId != null"> · run #{{ entry.agentRunId }}</template>
            </p>
          </div>
          <div class="flex flex-shrink-0 items-center gap-1">
            <UButton
              v-if="entry.status === 'proposed'"
              size="xs"
              color="primary"
              variant="soft"
              :icon-only="true"
              title="Approve"
              @click="curation.approve(entry.id)"
            >
              <UIcon name="i-lucide-check" class="h-3.5 w-3.5" />
            </UButton>
            <UButton
              v-if="entry.status === 'proposed'"
              size="xs"
              color="neutral"
              variant="soft"
              title="Reject"
              @click="curation.reject(entry.id)"
            >
              <UIcon name="i-lucide-x" class="h-3.5 w-3.5" />
            </UButton>
            <UButton
              v-if="entry.undoable"
              size="xs"
              color="neutral"
              variant="ghost"
              title="Undo"
              @click="curation.undo(entry.id)"
            >
              <UIcon name="i-lucide-rotate-ccw" class="h-3.5 w-3.5" />
            </UButton>
          </div>
        </li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * One action-log entry in the activity feed: who acted, what the action
 * says (with the proposed prose readable, since that is what a reviewer
 * judges), its status and time, and the approve/reject/undo controls the
 * entry's status and undoability allow. Dispatches through `useCuration`.
 */

import { useCuration } from '@/composables/useCuration';
import type { ActionLogEntry } from '@/types/generated';

defineProps<{
  entry: ActionLogEntry;
  /** Inside a run card the run number is already in the header. */
  showRun?: boolean;
}>();

const curation = useCuration();

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
  // The prose a describe run proposes is what a reviewer reads, so it gets more room.
  for (const key of ['description', 'display_name', 'displayName', 'role', 'polarity']) {
    const value = params[key];
    if (typeof value === 'string' && value.length > 0) {
      details.push(`${key}=${value.length > 80 ? `${value.slice(0, 80)}…` : value}`);
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
  <div class="flex items-start gap-3 px-4 py-3">
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
        <template v-if="showRun && entry.agentRunId != null">
          · run #{{ entry.agentRunId }}
        </template>
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
  </div>
</template>

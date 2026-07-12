<script setup lang="ts">
import { Bot, Check, RotateCcw, User, X } from '@lucide/vue';
import { computed, onMounted } from 'vue';

import { useCurationStore } from '@/stores/curation';
import type { ActionLogEntry } from '@/types/generated';

const curation = useCurationStore();

onMounted(() => {
  void curation.refreshFeed();
});

const entries = computed(() => curation.feed);

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
    <div class="border-b border-default px-4 py-3">
      <h2 class="text-sm font-semibold text-highlighted">Activity</h2>
      <p class="text-sm text-muted">Every curation action — by you or an agent.</p>
    </div>
    <div class="flex-1 overflow-y-auto">
      <p v-if="curation.feedLoading && entries.length === 0" class="p-4 text-sm text-muted">
        Loading…
      </p>
      <p v-else-if="entries.length === 0" class="p-4 text-sm text-muted">No actions yet.</p>
      <ul v-else class="divide-y divide-default">
        <li v-for="entry in entries" :key="entry.id" class="flex items-start gap-3 px-4 py-3">
          <component
            :is="entry.actorType === 'agent' ? Bot : User"
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
              <Check class="h-3.5 w-3.5" />
            </UButton>
            <UButton
              v-if="entry.status === 'proposed'"
              size="xs"
              color="neutral"
              variant="soft"
              title="Reject"
              @click="curation.reject(entry.id)"
            >
              <X class="h-3.5 w-3.5" />
            </UButton>
            <UButton
              v-if="entry.undoable"
              size="xs"
              color="neutral"
              variant="ghost"
              title="Undo"
              @click="curation.undo(entry.id)"
            >
              <RotateCcw class="h-3.5 w-3.5" />
            </UButton>
          </div>
        </li>
      </ul>
    </div>
  </div>
</template>

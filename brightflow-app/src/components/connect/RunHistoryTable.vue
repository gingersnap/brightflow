<script setup lang="ts">
import type { EnrichedSyncRun } from '@/types';

import SyncStatusBadge from './SyncStatusBadge.vue';

defineProps<{
  runs: EnrichedSyncRun[];
}>();

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  if (diff < 60_000) {
    return 'just now';
  }
  if (diff < 3_600_000) {
    return `${Math.round(diff / 60_000)}m ago`;
  }
  if (diff < 86_400_000) {
    return `${Math.round(diff / 3_600_000)}h ago`;
  }
  return `${Math.round(diff / 86_400_000)}d ago`;
}

function duration(startedAt: string, finishedAt: string): string {
  const ms = new Date(finishedAt).getTime() - new Date(startedAt).getTime();
  return `${Math.round(ms / 1000)}s`;
}
</script>

<template>
  <div v-if="runs.length === 0" class="py-4 text-center text-sm text-muted">No runs yet</div>
  <div v-else class="divide-y divide-default">
    <div
      v-for="run in runs"
      :key="run.id"
      class="flex flex-wrap items-center gap-x-2.5 gap-y-1 px-1 py-2 text-sm"
    >
      <SyncStatusBadge :status="run.status as 'pending' | 'running' | 'completed' | 'failed'" />
      <span class="font-medium text-highlighted">{{ run.connectorName }}</span>
      <span class="text-muted">{{ relativeTime(run.startedAt) }}</span>
      <span v-if="run.finishedAt" class="text-muted">
        ({{ duration(run.startedAt, run.finishedAt) }})
      </span>
      <span v-if="run.rowsSynced > 0" class="text-muted">{{ run.rowsSynced }} rows</span>
      <span v-if="run.status === 'failed' && run.error" class="break-all text-red-400">
        {{ run.error }}
      </span>
    </div>
  </div>
</template>

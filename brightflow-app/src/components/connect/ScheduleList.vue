<script setup lang="ts">
import { Play, Trash2 } from 'lucide-vue-next';
import { computed } from 'vue';

import type { UnifiedConnector } from '@/types';

const props = defineProps<{
  connectors: UnifiedConnector[];
}>();

const emit = defineEmits<{
  run: [name: string];
  delete: [jobId: string];
}>();

const scheduled = computed(() =>
  props.connectors.filter((c) => c.job && c.job.enabled && c.job.intervalSecs > 0),
);

function intervalLabel(secs: number): string {
  if (secs < 3600) {
    return `${Math.round(secs / 60)}m`;
  }
  if (secs < 86_400) {
    return `${Math.round(secs / 3600)}h`;
  }
  return `${Math.round(secs / 86_400)}d`;
}

function isRunning(c: UnifiedConnector): boolean {
  return c.lastRun != null && (c.lastRun.status === 'running' || c.lastRun.status === 'pending');
}

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

function relativeTimeUntil(ms: number): string {
  if (ms <= 0) {
    return 'soon';
  }
  if (ms < 60_000) {
    return 'in <1m';
  }
  if (ms < 3_600_000) {
    return `in ${Math.round(ms / 60_000)}m`;
  }
  if (ms < 86_400_000) {
    return `in ${Math.round(ms / 3_600_000)}h`;
  }
  return `in ${Math.round(ms / 86_400_000)}d`;
}

function nextRunLabel(c: UnifiedConnector): string {
  if (!c.job) {
    return '';
  }
  if (!c.lastRun?.startedAt) {
    return 'soon';
  }
  const lastStart = new Date(c.lastRun.startedAt).getTime();
  const nextAt = lastStart + c.job.intervalSecs * 1000;
  const remaining = nextAt - Date.now();
  return relativeTimeUntil(remaining);
}

function lastCompletedLabel(c: UnifiedConnector): string | null {
  if (!c.lastRun || c.lastRun.status !== 'completed') {
    return null;
  }
  const time = relativeTime(c.lastRun.startedAt);
  const rows = c.lastRun.rowsSynced > 0 ? `, ${c.lastRun.rowsSynced} rows` : '';
  return `${time}${rows}`;
}
</script>

<template>
  <div v-if="scheduled.length === 0" class="py-4 text-center text-sm text-muted">
    No active schedules
  </div>
  <div v-else class="divide-y divide-default">
    <div v-for="c in scheduled" :key="c.name" class="flex items-center gap-3 px-1 py-2.5 text-sm">
      <!-- Status dot: green = active, blue pulse = running -->
      <span
        class="h-2 w-2 shrink-0 rounded-full"
        :class="isRunning(c) ? 'animate-pulse bg-blue-500' : 'bg-green-500'"
      />

      <!-- Name -->
      <span class="min-w-0 truncate font-medium text-highlighted">{{ c.name }}</span>

      <!-- Interval -->
      <span class="rounded bg-elevated px-1.5 py-0.5 text-xs text-muted">
        every {{ intervalLabel(c.job!.intervalSecs) }}
      </span>

      <!-- Next run / last completed — subtle muted text -->
      <span class="hidden items-center gap-2 text-sm text-muted sm:flex">
        <span v-if="!isRunning(c)">Next {{ nextRunLabel(c) }}</span>
        <span v-if="!isRunning(c) && lastCompletedLabel(c)">
          &middot; Last {{ lastCompletedLabel(c) }}
        </span>
        <span v-if="isRunning(c)">Running&hellip;</span>
      </span>

      <div class="flex-1" />

      <!-- Run Now -->
      <UButton
        size="md"
        variant="ghost"
        :disabled="isRunning(c)"
        :loading="isRunning(c)"
        @click="emit('run', c.name)"
      >
        <Play v-if="!isRunning(c)" class="mr-1 h-3 w-3" />
        {{ isRunning(c) ? 'Running' : 'Run Now' }}
      </UButton>

      <!-- Delete -->
      <UButton size="md" variant="ghost" color="error" @click="emit('delete', c.job!.id)">
        <Trash2 class="h-3 w-3" />
      </UButton>
    </div>
  </div>
</template>

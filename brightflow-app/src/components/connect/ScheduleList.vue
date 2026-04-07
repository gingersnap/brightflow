<script setup lang="ts">
import { Play, Trash2 } from 'lucide-vue-next';
import { computed } from 'vue';

import type { UnifiedConnector } from '@/types';

import SyncStatusBadge from './SyncStatusBadge.vue';

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
</script>

<template>
  <div v-if="scheduled.length === 0" class="py-4 text-center text-xs text-muted">
    No active schedules
  </div>
  <div v-else class="divide-y divide-default">
    <div v-for="c in scheduled" :key="c.name" class="flex items-center gap-3 px-1 py-2.5 text-sm">
      <!-- Status dot -->
      <span
        class="h-2 w-2 shrink-0 rounded-full"
        :class="{
          'bg-green-500': c.lastRun?.status === 'completed',
          'bg-red-500': c.lastRun?.status === 'failed',
          'animate-pulse bg-blue-500': isRunning(c),
          'bg-neutral-400': !c.lastRun,
        }"
      />

      <!-- Name -->
      <span class="min-w-0 flex-1 truncate font-medium text-highlighted">{{ c.name }}</span>

      <!-- Interval badge -->
      <span class="rounded bg-elevated px-1.5 py-0.5 text-xs text-muted">
        every {{ intervalLabel(c.job!.intervalSecs) }}
      </span>

      <!-- Last run status -->
      <SyncStatusBadge
        v-if="c.lastRun"
        :status="c.lastRun.status as 'pending' | 'running' | 'completed' | 'failed'"
      />

      <!-- Run Now -->
      <UButton
        size="xs"
        variant="ghost"
        :disabled="isRunning(c)"
        :loading="isRunning(c)"
        @click="emit('run', c.name)"
      >
        <Play v-if="!isRunning(c)" class="mr-1 h-3 w-3" />
        {{ isRunning(c) ? 'Running' : 'Run Now' }}
      </UButton>

      <!-- Delete -->
      <UButton size="xs" variant="ghost" color="error" @click="emit('delete', c.job!.id)">
        <Trash2 class="h-3 w-3" />
      </UButton>
    </div>
  </div>
</template>

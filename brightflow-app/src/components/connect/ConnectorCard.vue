<script setup lang="ts">
import { Play, CheckCircle, XCircle, Clock, AlertTriangle, CalendarClock } from 'lucide-vue-next';
import type { ConnectorInfo, ConnectorRun } from '@/services/api';

defineProps<{
  connector: ConnectorInfo;
  running: boolean;
  lastRun: ConnectorRun | undefined;
}>();

const emit = defineEmits<{
  run: [];
  schedule: [intervalSecs: number];
}>();

const SCHEDULE_PRESETS = [
  { label: 'No schedule', value: 0 },
  { label: 'Every 1h', value: 3600 },
  { label: 'Every 6h', value: 21600 },
  { label: 'Every 12h', value: 43200 },
  { label: 'Daily', value: 86400 },
  { label: 'Weekly', value: 604800 },
];

function formatTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString();
}

function handleScheduleChange(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  emit('schedule', value);
}
</script>

<template>
  <div class="rounded-lg border border-default bg-default p-4">
    <div class="flex items-start justify-between">
      <div>
        <div class="flex items-center gap-2">
          <h3 class="font-medium text-highlighted">{{ connector.name }}</h3>
          <span
            v-if="!connector.valid"
            class="inline-flex items-center gap-1 text-xs text-amber-500"
          >
            <AlertTriangle class="w-3 h-3" />
            Missing connector
          </span>
        </div>
        <p class="text-xs text-muted mt-0.5">{{ connector.connector }}</p>
      </div>

      <div class="flex items-center gap-2">
        <UButton
          size="sm"
          :disabled="!connector.valid || running"
          :loading="running"
          @click="emit('run')"
        >
          <Play v-if="!running" class="w-3.5 h-3.5 mr-1" />
          {{ running ? 'Running...' : 'Run' }}
        </UButton>
      </div>
    </div>

    <!-- Schedule selector -->
    <div class="mt-3 pt-3 border-t border-default flex items-center gap-2">
      <CalendarClock class="w-3.5 h-3.5 text-muted" />
      <select
        class="text-xs bg-elevated border border-default rounded px-2 py-1 text-muted cursor-pointer"
        @change="handleScheduleChange"
      >
        <option v-for="preset in SCHEDULE_PRESETS" :key="preset.value" :value="preset.value">
          {{ preset.label }}
        </option>
      </select>
    </div>

    <!-- Last run status -->
    <div v-if="lastRun" class="mt-3 pt-3 border-t border-default">
      <div class="flex items-center gap-2 text-xs">
        <CheckCircle v-if="lastRun.status === 'completed'" class="w-3.5 h-3.5 text-green-500" />
        <XCircle v-else-if="lastRun.status === 'failed'" class="w-3.5 h-3.5 text-red-500" />
        <Clock v-else class="w-3.5 h-3.5 text-blue-500 animate-pulse" />

        <span class="capitalize text-muted">{{ lastRun.status }}</span>

        <span v-if="lastRun.finished_at" class="text-muted">
          {{ formatTime(lastRun.finished_at) }}
        </span>
      </div>

      <div v-if="lastRun.status === 'completed' && lastRun.tables_ingested.length > 0" class="mt-1.5">
        <span class="text-xs text-muted">Tables: </span>
        <span
          v-for="(table, i) in lastRun.tables_ingested"
          :key="table"
          class="text-xs text-highlighted"
        >{{ i > 0 ? ', ' : '' }}{{ table }}</span>
      </div>

      <div v-if="lastRun.status === 'failed' && lastRun.error" class="mt-1.5">
        <span class="text-xs text-red-500">{{ lastRun.error }}</span>
      </div>
    </div>
  </div>
</template>

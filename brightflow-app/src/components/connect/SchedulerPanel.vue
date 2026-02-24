<script setup lang="ts">
import { onMounted, computed } from 'vue';
import { Play, Trash2, Clock, RefreshCw } from 'lucide-vue-next';
import { useSchedulerStore } from '@/stores/scheduler';
import SyncStatusBadge from './SyncStatusBadge.vue';

const schedulerStore = useSchedulerStore();

const INTERVAL_PRESETS = [
  { label: 'Manual', value: 0 },
  { label: 'Every 1h', value: 3600 },
  { label: 'Every 6h', value: 21600 },
  { label: 'Every 12h', value: 43200 },
  { label: 'Daily', value: 86400 },
  { label: 'Weekly', value: 604800 },
];

onMounted(async () => {
  await Promise.all([schedulerStore.fetchJobs(), schedulerStore.fetchRuns()]);
});

function relativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = Date.now();
  const diff = now - date.getTime();

  if (diff < 60000) return 'just now';
  if (diff < 3600000) return `${Math.round(diff / 60000)}m ago`;
  if (diff < 86400000) return `${Math.round(diff / 3600000)}h ago`;
  return `${Math.round(diff / 86400000)}d ago`;
}

function handleTrigger(jobId: string): void {
  schedulerStore.triggerRun(jobId);
}

async function handleToggle(jobId: string, enabled: boolean): Promise<void> {
  await schedulerStore.updateJob(jobId, { enabled: !enabled });
}

async function handleDelete(jobId: string): Promise<void> {
  await schedulerStore.deleteJob(jobId);
}

async function handleIntervalChange(jobId: string, intervalSecs: number): Promise<void> {
  await schedulerStore.updateJob(jobId, { intervalSecs });
}

const recentRuns = computed(() => schedulerStore.runs.slice(0, 20));

async function refresh(): Promise<void> {
  await Promise.all([schedulerStore.fetchJobs(), schedulerStore.fetchRuns()]);
}
</script>

<template>
  <div class="space-y-6">
    <!-- Jobs Section -->
    <div>
      <div class="flex items-center gap-3 mb-3">
        <h3 class="text-sm font-semibold text-highlighted">Scheduled Jobs</h3>
        <div class="flex-1" />
        <UButton variant="ghost" size="xs" :loading="schedulerStore.loading" @click="refresh">
          <RefreshCw class="w-3 h-3 mr-1" />
          Refresh
        </UButton>
      </div>

      <div v-if="schedulerStore.error" class="mb-4 p-3 rounded-lg bg-red-500/10 text-red-500 text-sm">
        {{ schedulerStore.error }}
      </div>

      <div v-if="schedulerStore.jobs.length === 0" class="text-sm text-muted py-4 text-center">
        No scheduled jobs. Create a connector config and add a schedule.
      </div>

      <div v-else class="space-y-2">
        <div
          v-for="job in schedulerStore.jobs"
          :key="job.id"
          class="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-default bg-default"
        >
          <!-- Status dot -->
          <span
            class="h-2 w-2 rounded-full shrink-0"
            :class="job.enabled ? 'bg-green-500' : 'bg-neutral-400'"
          />

          <!-- Job name -->
          <span class="font-medium text-sm text-highlighted">{{ job.name }}</span>

          <!-- Interval selector -->
          <select
            class="text-xs bg-elevated border border-default rounded px-2 py-1 text-muted"
            :value="job.intervalSecs"
            @change="handleIntervalChange(job.id, Number(($event.target as HTMLSelectElement).value))"
          >
            <option v-for="preset in INTERVAL_PRESETS" :key="preset.value" :value="preset.value">
              {{ preset.label }}
            </option>
          </select>

          <!-- Last sync info -->
          <template v-if="schedulerStore.latestRunForJob(job.id)">
            <SyncStatusBadge :status="schedulerStore.latestRunForJob(job.id)!.status" />
            <span class="text-xs text-muted">
              <Clock class="w-3 h-3 inline mr-0.5" />
              {{ relativeTime(schedulerStore.latestRunForJob(job.id)!.startedAt) }}
            </span>
          </template>
          <span v-else class="text-xs text-muted">Never synced</span>

          <div class="flex-1" />

          <!-- Actions -->
          <button
            class="text-xs text-muted hover:text-highlighted transition-colors cursor-pointer"
            @click="handleToggle(job.id, job.enabled)"
          >
            {{ job.enabled ? 'Disable' : 'Enable' }}
          </button>

          <UButton variant="ghost" size="xs" @click="handleTrigger(job.id)">
            <Play class="w-3 h-3 mr-1" />
            Sync Now
          </UButton>

          <UButton variant="ghost" size="xs" color="red" @click="handleDelete(job.id)">
            <Trash2 class="w-3 h-3" />
          </UButton>
        </div>
      </div>
    </div>

    <!-- Run History Section -->
    <div v-if="recentRuns.length > 0">
      <h3 class="text-sm font-semibold text-highlighted mb-3">Recent Sync Runs</h3>
      <div class="space-y-1.5">
        <div
          v-for="run in recentRuns"
          :key="run.id"
          class="flex items-center gap-3 px-3 py-2 rounded-lg border border-default bg-default text-sm"
        >
          <SyncStatusBadge :status="run.status" />

          <span class="text-xs text-muted font-mono">{{ run.connectorId.slice(0, 8) }}...</span>

          <span v-if="run.rowsSynced > 0" class="text-xs text-muted">
            {{ run.rowsSynced }} rows
          </span>

          <span v-if="run.error" class="text-xs text-red-400 truncate max-w-48" :title="run.error">
            {{ run.error }}
          </span>

          <div class="flex-1" />

          <span class="text-xs text-muted">
            {{ relativeTime(run.startedAt) }}
          </span>

          <span v-if="run.finishedAt" class="text-xs text-muted">
            ({{ Math.round((new Date(run.finishedAt).getTime() - new Date(run.startedAt).getTime()) / 1000) }}s)
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

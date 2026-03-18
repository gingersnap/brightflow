<script setup lang="ts">
import { computed, ref } from 'vue';
import { Play, AlertTriangle, ChevronRight, ChevronDown, Key } from 'lucide-vue-next';
import type { UnifiedConnector, SyncRun } from '@/services/api';
import SyncStatusBadge from './SyncStatusBadge.vue';

const props = defineProps<{
  connector: UnifiedConnector;
  running: boolean;
  expanded: boolean;
  history: SyncRun[];
}>();

const emit = defineEmits<{
  run: [];
  schedule: [intervalSecs: number];
  toggleHistory: [];
  updateToken: [token: string];
}>();

const showTokenInput = ref(false);
const tokenValue = ref('');
const savingToken = ref(false);

function toggleTokenInput(): void {
  showTokenInput.value = !showTokenInput.value;
  tokenValue.value = '';
}

async function saveToken(): Promise<void> {
  if (!tokenValue.value.trim()) return;
  savingToken.value = true;
  emit('updateToken', tokenValue.value.trim());
  savingToken.value = false;
  showTokenInput.value = false;
  tokenValue.value = '';
}

const SCHEDULE_PRESETS = [
  { label: 'Manual', value: 0 },
  { label: 'Every 1h', value: 3600 },
  { label: 'Every 6h', value: 21600 },
  { label: 'Every 12h', value: 43200 },
  { label: 'Daily', value: 86400 },
  { label: 'Weekly', value: 604800 },
];

const currentInterval = computed(() => props.connector.job?.intervalSecs ?? 0);

function handleScheduleChange(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  emit('schedule', value);
}

function relativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = Date.now();
  const diff = now - date.getTime();

  if (diff < 60000) return 'just now';
  if (diff < 3600000) return `${Math.round(diff / 60000)}m ago`;
  if (diff < 86400000) return `${Math.round(diff / 3600000)}h ago`;
  return `${Math.round(diff / 86400000)}d ago`;
}

function duration(startedAt: string, finishedAt: string): string {
  const ms = new Date(finishedAt).getTime() - new Date(startedAt).getTime();
  const secs = Math.round(ms / 1000);
  return `${secs}s`;
}
</script>

<template>
  <div class="rounded-lg border border-default bg-default">
    <!-- Main row -->
    <div class="flex items-center gap-3 px-4 py-3">
      <!-- Status dot -->
      <span
        class="h-2.5 w-2.5 rounded-full shrink-0"
        :class="{
          'bg-green-500': connector.lastRun?.status === 'completed',
          'bg-red-500': connector.lastRun?.status === 'failed',
          'bg-blue-500 animate-pulse': running,
          'bg-neutral-400': !connector.lastRun && !running,
        }"
      />

      <!-- Connector name -->
      <div class="min-w-0">
        <div class="flex items-center gap-1.5">
          <span class="font-medium text-highlighted text-sm">{{ connector.name }}</span>
          <span
            v-if="!connector.valid"
            class="inline-flex items-center gap-0.5 text-xs text-amber-500"
          >
            <AlertTriangle class="w-3 h-3" />
            missing
          </span>
        </div>
      </div>

      <!-- Token status -->
      <button
        class="inline-flex items-center gap-1 text-xs px-1.5 py-0.5 rounded cursor-pointer transition-colors"
        :class="connector.hasToken
          ? 'text-green-500 hover:text-green-400'
          : 'text-amber-500 hover:text-amber-400'"
        :title="connector.hasToken ? 'Token configured — click to update' : 'No token set — click to add'"
        @click="toggleTokenInput"
      >
        <Key class="w-3 h-3" />
        {{ connector.hasToken ? 'Token set' : 'No token' }}
      </button>

      <div class="flex-1" />

      <!-- Schedule dropdown -->
      <select
        class="text-xs bg-elevated border border-default rounded px-2 py-1 text-muted cursor-pointer"
        :value="currentInterval"
        @change="handleScheduleChange"
      >
        <option v-for="preset in SCHEDULE_PRESETS" :key="preset.value" :value="preset.value">
          {{ preset.label }}
        </option>
      </select>

      <!-- Sync Now button -->
      <UButton
        size="sm"
        :disabled="!connector.valid || running"
        :loading="running"
        @click="emit('run')"
      >
        <Play v-if="!running" class="w-3.5 h-3.5 mr-1" />
        {{ running ? 'Syncing...' : 'Sync Now' }}
      </UButton>
    </div>

    <!-- Token input -->
    <div v-if="showTokenInput" class="px-4 pb-2 flex items-center gap-2">
      <input
        v-model="tokenValue"
        type="password"
        placeholder="Paste API token..."
        class="flex-1 text-xs bg-elevated border border-default rounded px-2 py-1.5 text-highlighted
               placeholder-muted focus:outline-none focus:border-blue-500"
        @keyup.enter="saveToken"
      >
      <UButton size="xs" :loading="savingToken" :disabled="!tokenValue.trim()" @click="saveToken">
        Save
      </UButton>
      <UButton size="xs" variant="ghost" @click="toggleTokenInput">
        Cancel
      </UButton>
    </div>

    <!-- Last sync summary -->
    <div
      v-if="connector.lastRun || running"
      class="px-4 pb-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted"
    >
      <template v-if="running && !connector.lastRun">
        <span class="text-blue-500">Sync in progress...</span>
      </template>
      <template v-else-if="connector.lastRun">
        <SyncStatusBadge :status="connector.lastRun.status" />
        <span>{{ relativeTime(connector.lastRun.startedAt) }}</span>
        <span v-if="connector.lastRun.finishedAt">
          ({{ duration(connector.lastRun.startedAt, connector.lastRun.finishedAt) }})
        </span>
        <span v-if="connector.lastRun.rowsSynced > 0">
          {{ connector.lastRun.rowsSynced }} rows
        </span>
        <span
          v-if="connector.lastRun.status === 'failed' && connector.lastRun.error"
          class="text-red-400 break-all"
        >
          {{ connector.lastRun.error }}
        </span>
      </template>
    </div>

    <!-- Run History toggle -->
    <button
      class="w-full flex items-center gap-1.5 px-4 py-2 text-xs text-muted hover:text-highlighted
             border-t border-default transition-colors cursor-pointer"
      @click="emit('toggleHistory')"
    >
      <ChevronDown v-if="expanded" class="w-3 h-3" />
      <ChevronRight v-else class="w-3 h-3" />
      Run History
    </button>

    <!-- Expanded history -->
    <div v-if="expanded" class="border-t border-default">
      <div v-if="history.length === 0" class="px-4 py-3 text-xs text-muted text-center">
        No runs yet
      </div>
      <div v-else class="divide-y divide-default">
        <div
          v-for="run in history"
          :key="run.id"
          class="flex flex-wrap items-center gap-x-2.5 gap-y-1 px-4 py-2 text-xs"
        >
          <SyncStatusBadge :status="run.status" />
          <span class="text-muted">{{ relativeTime(run.startedAt) }}</span>
          <span v-if="run.finishedAt" class="text-muted">
            ({{ duration(run.startedAt, run.finishedAt) }})
          </span>
          <span v-if="run.rowsSynced > 0" class="text-muted">
            {{ run.rowsSynced }} rows
          </span>
          <span
            v-if="run.status === 'failed' && run.error"
            class="text-red-400 break-all"
          >
            {{ run.error }}
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

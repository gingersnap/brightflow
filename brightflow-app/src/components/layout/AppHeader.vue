<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { useColorMode } from '@vueuse/core';
import { Activity, Cable, LogOut, Moon, Sun } from 'lucide-vue-next';
import { computed } from 'vue';

import SourceSwitcher from '@/components/layout/SourceSwitcher.vue';
import { connectApi } from '@/services/api';
import { useSourceStore } from '@/stores/source';
import { useUiStore } from '@/stores/ui';
import type { UnifiedConnector } from '@/types';

const uiStore = useUiStore();
const sourceStore = useSourceStore();
const colorMode = useColorMode();

const { data: connectors } = useQuery({
  key: ['connectors'],
  query: async () => {
    const result = await connectApi.listUnified();
    return result ?? ([] as UnifiedConnector[]);
  },
});

const lastSyncTime = computed(() => {
  const latest = (connectors.value ?? [])
    .map((c) => c.lastRun)
    .filter(
      (r): r is NonNullable<typeof r> =>
        r != null && r.status === 'completed' && r.finishedAt != null,
    )
    .toSorted((a, b) => new Date(b.finishedAt!).getTime() - new Date(a.finishedAt!).getTime())[0];

  if (!latest?.finishedAt) {
    return null;
  }

  const date = new Date(latest.finishedAt);
  const now = Date.now();
  const diff = now - date.getTime();

  if (diff < 60_000) {
    return 'just now';
  }
  if (diff < 3_600_000) {
    return `${Math.round(diff / 60_000)}m ago`;
  }
  if (diff < 86_400_000) {
    return `${Math.round(diff / 3_600_000)}h ago`;
  }
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
});

const emit = defineEmits<{
  logout: [];
}>();

const periods = [
  { label: 'Today', value: 'today' },
  { label: '7 days', value: '7d' },
  { label: '30 days', value: '30d' },
  { label: 'This month', value: 'month' },
  { label: '12 months', value: '12m' },
];

const showPeriod = computed(
  () =>
    sourceStore.selectedSource != null &&
    ['dashboard', 'funnels', 'retention', 'events'].includes(sourceStore.selectedTool),
);

function toggleConnect(): void {
  uiStore.setShowConnect(!uiStore.showConnect);
}

function toggleSystem(): void {
  uiStore.setShowSystem(!uiStore.showSystem);
}

function toggleTheme(): void {
  colorMode.value = colorMode.value === 'dark' ? 'light' : 'dark';
}
</script>

<template>
  <div class="flex h-14 items-center justify-between border-b border-default bg-default px-4">
    <!-- Left: Logo and Source Switcher -->
    <div class="flex items-center gap-4">
      <h1 class="text-lg font-semibold text-highlighted">Brightflow</h1>

      <!-- Source switcher (when a source is selected) -->
      <SourceSwitcher v-if="sourceStore.selectedSource" />

      <!-- Period selector -->
      <div v-if="showPeriod" class="ml-2 flex items-center gap-1 rounded-lg bg-elevated p-0.5">
        <button
          v-for="p in periods"
          :key="p.value"
          class="rounded-md px-3 py-1 text-xs font-medium transition-colors"
          :class="
            sourceStore.period === p.value
              ? 'bg-default text-highlighted shadow-sm'
              : 'cursor-pointer text-muted hover:text-highlighted'
          "
          @click="sourceStore.period = p.value"
        >
          {{ p.label }}
        </button>
      </div>
    </div>

    <!-- Right: Actions and Status -->
    <div class="flex items-center gap-3">
      <!-- Connect button -->
      <button
        class="flex cursor-pointer items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors"
        :class="
          uiStore.showConnect
            ? 'bg-primary-500/10 text-primary-500'
            : 'text-muted hover:bg-elevated hover:text-highlighted'
        "
        @click="toggleConnect"
      >
        <Cable class="h-3.5 w-3.5" />
        Connect
      </button>

      <!-- System button -->
      <button
        class="flex cursor-pointer items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors"
        :class="
          uiStore.showSystem
            ? 'bg-primary-500/10 text-primary-500'
            : 'text-muted hover:bg-elevated hover:text-highlighted'
        "
        @click="toggleSystem"
      >
        <Activity class="h-3.5 w-3.5" />
        System
      </button>

      <!-- Theme toggle -->
      <UButton variant="ghost" size="sm" square @click="toggleTheme">
        <Sun v-if="colorMode === 'dark'" class="h-4 w-4" />
        <Moon v-else class="h-4 w-4" />
      </UButton>

      <!-- Logout button -->
      <UButton variant="ghost" size="sm" square @click="emit('logout')">
        <LogOut class="h-4 w-4" />
      </UButton>

      <!-- Sync status dot -->
      <div class="flex items-center gap-2 border-l border-default pl-3">
        <span
          class="h-2 w-2 rounded-full"
          :class="{
            'bg-green-500': sourceStore.selectedSource != null,
            'bg-neutral-400': sourceStore.selectedSource == null,
          }"
        />
        <span class="text-xs text-muted">
          {{ sourceStore.selectedSource?.name ?? 'No source' }}
          <span v-if="lastSyncTime" class="ml-1 opacity-70"> · synced {{ lastSyncTime }} </span>
        </span>
      </div>
    </div>
  </div>
</template>

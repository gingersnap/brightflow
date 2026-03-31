<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { useColorMode } from '@vueuse/core';
import {
  Activity,
  Cable,
  ChevronDown,
  Database,
  LogOut,
  Moon,
  Plus,
  Search,
  Sparkles,
  Sun,
} from 'lucide-vue-next';
import { computed } from 'vue';

import { connectApi } from '@/services/api';
import { useDatasetStore } from '@/stores/dataset';
import { type AppMode, useUiStore } from '@/stores/ui';
import type { UnifiedConnector } from '@/types';

const datasetStore = useDatasetStore();
const uiStore = useUiStore();
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

defineProps<{
  currentDataset: string | null;
}>();

const emit = defineEmits<{
  'change-dataset': [];
  logout: [];
}>();

function setMode(mode: AppMode): void {
  uiStore.setAppMode(mode);
}

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
    <!-- Left: Logo and Current Dataset -->
    <div class="flex items-center gap-4">
      <h1 class="text-lg font-semibold text-highlighted">Brightflow</h1>

      <!-- Current dataset display with change button -->
      <button
        v-if="currentDataset"
        class="flex cursor-pointer items-center gap-2 rounded-lg border border-default px-3 py-1.5 transition-colors hover:border-primary-500/50 hover:bg-elevated"
        @click="emit('change-dataset')"
      >
        <Database class="h-4 w-4 text-muted" />
        <span class="font-medium">{{ datasetStore.name ?? currentDataset }}</span>
        <span v-if="datasetStore.rowCount" class="text-xs text-muted">
          ({{ datasetStore.rowCount.toLocaleString() }} rows)
        </span>
        <ChevronDown class="h-4 w-4 text-muted" />
      </button>

      <!-- Select Dataset button when no dataset is loaded -->
      <button
        v-else
        class="flex cursor-pointer items-center gap-2 rounded-lg border border-dashed border-default px-3 py-1.5 text-muted transition-colors hover:border-primary-500/50 hover:text-highlighted"
        @click="emit('change-dataset')"
      >
        <Plus class="h-4 w-4" />
        <span class="text-sm">Select Dataset</span>
      </button>

      <!-- Mode switcher (disabled when no dataset) -->
      <div
        class="ml-2 flex items-center gap-1 rounded-lg bg-elevated p-0.5 transition-opacity"
        :class="{ 'pointer-events-none opacity-40': !currentDataset }"
      >
        <button
          class="flex cursor-pointer items-center gap-1.5 rounded-md px-3 py-1 text-xs font-medium transition-colors"
          :class="
            uiStore.appMode === 'explore' && !uiStore.showConnect && !uiStore.showSystem
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="setMode('explore')"
        >
          <Search class="h-3.5 w-3.5" />
          Explore
        </button>
        <button
          class="flex cursor-pointer items-center gap-1.5 rounded-md px-3 py-1 text-xs font-medium transition-colors"
          :class="
            uiStore.appMode === 'insights' && !uiStore.showConnect && !uiStore.showSystem
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="setMode('insights')"
        >
          <Sparkles class="h-3.5 w-3.5" />
          Insights
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

      <!-- Dataset Status Dot -->
      <div class="flex items-center gap-2 border-l border-default pl-3">
        <span
          class="h-2 w-2 rounded-full"
          :class="{
            'bg-green-500': datasetStore.hasData,
            'animate-pulse bg-yellow-500': datasetStore.loading,
            'bg-neutral-400': !datasetStore.hasData && !datasetStore.loading,
          }"
        />
        <span class="text-xs text-muted">
          {{ datasetStore.hasData ? (datasetStore.name ?? 'Dataset loaded') : 'No dataset' }}
          <span v-if="lastSyncTime" class="ml-1 opacity-70"> · synced {{ lastSyncTime }} </span>
        </span>
      </div>
    </div>
  </div>
</template>

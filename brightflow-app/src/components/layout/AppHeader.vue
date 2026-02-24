<script setup lang="ts">
import { computed } from 'vue';
import {
  Database,
  ChevronDown,
  Search,
  Sparkles,
  Cable,
  Sun,
  Moon,
  Plus,
  LogOut,
  Zap,
  HardDrive,
} from 'lucide-vue-next';
import { useColorMode } from '@vueuse/core';
import { useDatasetStore } from '@/stores/dataset';
import { useAuthStore, type DataMode } from '@/stores/auth';
import { useUiStore, type AppMode } from '@/stores/ui';
import { useSchedulerStore } from '@/stores/scheduler';

const datasetStore = useDatasetStore();
const authStore = useAuthStore();
const uiStore = useUiStore();
const schedulerStore = useSchedulerStore();
const colorMode = useColorMode();

const lastSyncTime = computed(() => {
  const latestRun = schedulerStore.runs.find((r) => r.status === 'completed');
  if (!latestRun?.finishedAt) return null;

  const date = new Date(latestRun.finishedAt);
  const now = Date.now();
  const diff = now - date.getTime();

  if (diff < 60000) return 'just now';
  if (diff < 3600000) return `${Math.round(diff / 60000)}m ago`;
  if (diff < 86400000) return `${Math.round(diff / 3600000)}h ago`;
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

function toggleTheme(): void {
  colorMode.value = colorMode.value === 'dark' ? 'light' : 'dark';
}

async function toggleDataMode(): Promise<void> {
  const newMode: DataMode = authStore.dataMode === 'memory' ? 'lazy' : 'memory';
  await authStore.setDataMode(newMode);
}
</script>

<template>
  <div class="flex h-14 items-center justify-between px-4 border-b border-default bg-default">
    <!-- Left: Logo and Current Dataset -->
    <div class="flex items-center gap-4">
      <h1 class="text-lg font-semibold text-highlighted">Brightflow</h1>

      <!-- Current dataset display with change button -->
      <button
        v-if="currentDataset"
        class="flex items-center gap-2 px-3 py-1.5 rounded-lg border border-default hover:border-primary-500/50 hover:bg-elevated transition-colors cursor-pointer"
        @click="emit('change-dataset')"
      >
        <Database class="w-4 h-4 text-muted" />
        <span class="font-medium">{{ datasetStore.name ?? currentDataset }}</span>
        <span v-if="datasetStore.rowCount" class="text-xs text-muted">
          ({{ datasetStore.rowCount.toLocaleString() }} rows)
        </span>
        <ChevronDown class="w-4 h-4 text-muted" />
      </button>

      <!-- Select Dataset button when no dataset is loaded -->
      <button
        v-else
        class="flex items-center gap-2 px-3 py-1.5 rounded-lg border border-dashed border-default text-muted hover:border-primary-500/50 hover:text-highlighted transition-colors cursor-pointer"
        @click="emit('change-dataset')"
      >
        <Plus class="w-4 h-4" />
        <span class="text-sm">Select Dataset</span>
      </button>

      <!-- Mode switcher (disabled when no dataset) -->
      <div
        class="flex items-center gap-1 bg-elevated rounded-lg p-0.5 ml-2 transition-opacity"
        :class="{ 'opacity-40 pointer-events-none': !currentDataset }"
      >
        <button
          class="flex items-center gap-1.5 px-3 py-1 text-xs font-medium rounded-md transition-colors cursor-pointer"
          :class="
            uiStore.appMode === 'explore' && !uiStore.showConnect
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="setMode('explore')"
        >
          <Search class="w-3.5 h-3.5" />
          Explore
        </button>
        <button
          class="flex items-center gap-1.5 px-3 py-1 text-xs font-medium rounded-md transition-colors cursor-pointer"
          :class="
            uiStore.appMode === 'insights' && !uiStore.showConnect
              ? 'bg-default text-highlighted shadow-sm'
              : 'text-muted hover:text-highlighted'
          "
          @click="setMode('insights')"
        >
          <Sparkles class="w-3.5 h-3.5" />
          Insights
        </button>
      </div>
    </div>

    <!-- Right: Actions and Status -->
    <div class="flex items-center gap-3">
      <!-- Connect button -->
      <button
        class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg transition-colors cursor-pointer"
        :class="
          uiStore.showConnect
            ? 'bg-primary-500/10 text-primary-500'
            : 'text-muted hover:text-highlighted hover:bg-elevated'
        "
        @click="toggleConnect"
      >
        <Cable class="w-3.5 h-3.5" />
        Connect
      </button>

      <!-- Data mode toggle -->
      <button
        class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg transition-colors cursor-pointer"
        :class="
          authStore.dataMode === 'lazy'
            ? 'bg-primary-500/10 text-primary-500'
            : 'text-muted hover:text-highlighted hover:bg-elevated'
        "
        :title="authStore.dataMode === 'memory' ? 'Mode: In-Memory (click for Lazy)' : 'Mode: Lazy Scan (click for In-Memory)'"
        @click="toggleDataMode"
      >
        <Zap v-if="authStore.dataMode === 'lazy'" class="w-3.5 h-3.5" />
        <HardDrive v-else class="w-3.5 h-3.5" />
        {{ authStore.dataMode === 'lazy' ? 'Lazy' : 'Memory' }}
      </button>

      <!-- Theme toggle -->
      <UButton
        variant="ghost"
        size="sm"
        square
        @click="toggleTheme"
      >
        <Sun v-if="colorMode === 'dark'" class="w-4 h-4" />
        <Moon v-else class="w-4 h-4" />
      </UButton>

      <!-- Logout button -->
      <UButton
        variant="ghost"
        size="sm"
        square
        @click="emit('logout')"
      >
        <LogOut class="w-4 h-4" />
      </UButton>

      <!-- Dataset Status Dot -->
      <div class="flex items-center gap-2 pl-3 border-l border-default">
        <span
          class="h-2 w-2 rounded-full"
          :class="{
            'bg-green-500': datasetStore.hasData,
            'bg-yellow-500 animate-pulse': datasetStore.loading,
            'bg-neutral-400': !datasetStore.hasData && !datasetStore.loading
          }"
        />
        <span class="text-xs text-muted">
          {{ datasetStore.hasData ? (datasetStore.name ?? 'Dataset loaded') : 'No dataset' }}
          <span v-if="datasetStore.hasData && datasetStore.dataMode" class="ml-1 opacity-70">
            [{{ datasetStore.dataMode }}]
          </span>
          <span v-if="lastSyncTime" class="ml-1 opacity-70">
            · synced {{ lastSyncTime }}
          </span>
        </span>
      </div>
    </div>
  </div>
</template>

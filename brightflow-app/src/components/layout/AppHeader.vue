<script setup lang="ts">
import { useColorMode } from '@vueuse/core';
import { Activity, Cable, Layers, LogOut, Moon, Sun } from 'lucide-vue-next';

import { useSourceStore } from '@/stores/source';
import { useUiStore } from '@/stores/ui';

const uiStore = useUiStore();
const sourceStore = useSourceStore();
const colorMode = useColorMode();

const emit = defineEmits<{
  logout: [];
}>();

function toggleConnect(): void {
  uiStore.setShowConnect(!uiStore.showConnect);
}

function toggleSystem(): void {
  uiStore.setShowSystem(!uiStore.showSystem);
}

function toggleTheme(): void {
  colorMode.value = colorMode.value === 'dark' ? 'light' : 'dark';
}

function goToSources(): void {
  uiStore.setShowConnect(false);
  uiStore.setShowSystem(false);
  sourceStore.clearSource();
}

function goToSource(): void {
  uiStore.setShowConnect(false);
  uiStore.setShowSystem(false);
  sourceStore.selectTool('dashboard');
}
</script>

<template>
  <div class="flex h-14 items-center justify-between border-b border-default bg-default px-4">
    <!-- Left: Logo, Sources, Loaded data -->
    <div class="flex items-center gap-3">
      <h1 class="text-lg font-semibold text-highlighted">Brightflow</h1>

      <!-- Sources button -->
      <button
        class="flex cursor-pointer items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors"
        :class="
          !sourceStore.selectedSource && !uiStore.showConnect && !uiStore.showSystem
            ? 'bg-primary-500/10 text-primary-500'
            : 'text-muted hover:bg-elevated hover:text-highlighted'
        "
        @click="goToSources"
      >
        <Layers class="h-3.5 w-3.5" />
        Sources
      </button>

      <!-- Loaded data button -->
      <button
        v-if="sourceStore.selectedSource"
        class="flex cursor-pointer items-center gap-1.5 rounded-lg border border-default px-3 py-1.5 text-xs font-medium transition-colors hover:border-primary-500/50 hover:bg-elevated"
        @click="goToSource"
      >
        <span class="text-highlighted">{{ sourceStore.selectedSource.name }}</span>
      </button>
      <span v-else class="px-3 py-1.5 text-xs text-muted">No data loaded</span>
    </div>

    <!-- Right: Actions -->
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
    </div>
  </div>
</template>

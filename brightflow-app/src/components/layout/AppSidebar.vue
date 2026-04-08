<script setup lang="ts">
import {
  ArrowLeft,
  BarChart3,
  CalendarCheck,
  GitBranch,
  Search,
  Sparkles,
  Users,
} from 'lucide-vue-next';
import { type Component } from 'vue';

import { useSourceStore } from '@/stores/source';
import type { ToolId } from '@/types';

const sourceStore = useSourceStore();

const iconMap: Record<string, Component> = {
  BarChart3,
  GitBranch,
  CalendarCheck,
  Users,
  Search,
  Sparkles,
};
</script>

<template>
  <nav class="flex w-48 shrink-0 flex-col border-r border-default bg-default">
    <!-- Tool nav items -->
    <div class="flex flex-1 flex-col gap-0.5 p-2">
      <button
        v-for="tool in sourceStore.availableTools"
        :key="tool.id"
        class="flex cursor-pointer items-center gap-2.5 rounded-lg px-3 py-2 text-sm font-medium transition-colors"
        :class="
          sourceStore.selectedTool === tool.id
            ? 'bg-primary-500/10 text-primary-500'
            : 'text-muted hover:bg-elevated hover:text-highlighted'
        "
        @click="sourceStore.selectTool(tool.id as ToolId)"
      >
        <component :is="iconMap[tool.icon]" class="h-4 w-4" />
        {{ tool.label }}
      </button>
    </div>

    <!-- Bottom: source info + back link -->
    <div class="border-t border-default p-3">
      <p class="truncate text-xs font-medium text-highlighted">
        {{ sourceStore.selectedSource?.name }}
      </p>
      <button
        class="mt-1 flex cursor-pointer items-center gap-1 text-xs text-muted transition-colors hover:text-highlighted"
        @click="sourceStore.clearSource()"
      >
        <ArrowLeft class="h-3 w-3" />
        All sources
      </button>
    </div>
  </nav>
</template>

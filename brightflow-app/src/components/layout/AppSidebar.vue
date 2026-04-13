<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { useColorMode } from '@vueuse/core';
import { computed, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { sourceApi } from '@/services/api';
import { useAuthStore } from '@/stores/auth';
import { useSourceStore } from '@/stores/source';
import { type ToolId, type UnifiedSource, toolsForSource } from '@/types';

const router = useRouter();
const route = useRoute();
const sourceStore = useSourceStore();
const authStore = useAuthStore();
const colorMode = useColorMode();

const open = ref(false);

const emit = defineEmits<{
  logout: [];
}>();

// Fetch sources so sidebar always has them
useQuery({
  key: ['unified-sources'],
  query: async () => {
    const result = await sourceApi.unifiedList();
    const data = result ?? ([] as UnifiedSource[]);
    sourceStore.setSourcesData(data);
    return data;
  },
});

// Build source nav items with collapsible tool children
const sourceNavItems = computed(() =>
  sourceStore.sourcesData.map((source) => {
    const tools = toolsForSource(source);
    const currentSourceId = route.params.sourceId as string | undefined;
    return {
      label: source.name,
      icon: source.kind === 'web-analytics' ? 'i-lucide-globe' : 'i-lucide-cable',
      value: source.id,
      type: 'trigger' as const,
      defaultOpen: source.id === currentSourceId,
      children: tools.map((tool) => ({
        label: tool.label,
        icon: tool.icon,
        value: `${source.id}:${tool.id}`,
        onSelect: () => {
          handleSourceToolSelect(source.id, tool.id);
          open.value = false;
        },
      })),
    };
  }),
);

// Bottom navigation items
const bottomNavItems = computed(() => [
  {
    label: 'Sources',
    icon: 'i-lucide-layers',
    value: 'sources',
    onSelect: () => {
      goToSources();
      open.value = false;
    },
  },
  {
    label: 'System',
    icon: 'i-lucide-activity',
    value: 'system',
    onSelect: () => {
      toggleSystem();
      open.value = false;
    },
  },
]);

// Active bottom nav value
const BOTTOM_NAV_NAMES = new Set(['sources', 'system']);
const activeBottomValue = computed(() => {
  const name = typeof route.name === 'string' ? route.name : '';
  return BOTTOM_NAV_NAMES.has(name) ? name : undefined; // oxlint-disable-line no-useless-undefined
});

// User dropdown menu items
const userMenuItems = computed(() => [
  [
    {
      label: colorMode.value === 'dark' ? 'Light mode' : 'Dark mode',
      icon: colorMode.value === 'dark' ? 'i-lucide-sun' : 'i-lucide-moon',
      onSelect: () => {
        colorMode.value = colorMode.value === 'dark' ? 'light' : 'dark';
      },
    },
  ],
  [
    {
      label: 'Logout',
      icon: 'i-lucide-log-out',
      onSelect: () => emit('logout'),
    },
  ],
]);

function handleSourceToolSelect(sourceId: string, toolId: ToolId): void {
  router.push({ name: 'source-tool', params: { sourceId, tool: toolId } });
}

function goToSources(): void {
  router.push({ name: 'sources' });
}

function toggleSystem(): void {
  if (route.name === 'system') {
    router.push({ name: 'sources' });
  } else {
    router.push({ name: 'system' });
  }
}
</script>

<template>
  <UDashboardSidebar
    id="default"
    v-model:open="open"
    collapsible
    resizable
    class="bg-elevated/25"
    :ui="{ footer: 'lg:border-t lg:border-default' }"
  >
    <!-- Header: logo -->
    <template #header="{ collapsed }">
      <div class="flex items-center gap-2" :class="collapsed ? 'justify-center' : ''">
        <UIcon name="i-lucide-zap" class="h-5 w-5 shrink-0 text-primary-500" />
        <span v-if="!collapsed" class="text-lg font-semibold text-highlighted">Brightflow</span>
      </div>
    </template>

    <!-- Body: source tree + bottom nav -->
    <template #default="{ collapsed }">
      <!-- Sources with collapsible tool children -->
      <UNavigationMenu
        :collapsed="collapsed"
        :items="sourceNavItems"
        orientation="vertical"
        highlight
        tooltip
        popover
      />

      <!-- Bottom nav (pushed down) -->
      <UNavigationMenu
        :collapsed="collapsed"
        :items="bottomNavItems"
        orientation="vertical"
        color="neutral"
        :model-value="activeBottomValue"
        tooltip
        class="mt-auto"
      />
    </template>

    <!-- Footer: user menu -->
    <template #footer="{ collapsed }">
      <UDropdownMenu :items="userMenuItems" :content="{ side: 'right', align: 'end' }">
        <button
          class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-sm transition-colors hover:bg-elevated"
          :class="collapsed ? 'justify-center' : ''"
        >
          <UAvatar :text="authStore.user?.displayName?.charAt(0) ?? '?'" size="2xs" />
          <span v-if="!collapsed" class="truncate text-sm text-highlighted">
            {{ authStore.user?.displayName ?? 'User' }}
          </span>
        </button>
      </UDropdownMenu>
    </template>
  </UDashboardSidebar>
</template>

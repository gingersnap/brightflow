<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { useColorMode } from '@vueuse/core';
import { computed, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { sourceApi } from '@/services/api';
import { useAuthStore } from '@/stores/auth';
import { useSourceStore } from '@/stores/source';
import { useUiStore } from '@/stores/ui';
import { type ToolId, type UnifiedSource, toolsForSource } from '@/types';

const router = useRouter();
const route = useRoute();
const sourceStore = useSourceStore();
const authStore = useAuthStore();
const uiStore = useUiStore();
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

// Build nav items: Sources group + Settings group
const navItems = computed(() => {
  const currentSourceId = route.params.sourceId as string | undefined;
  const currentTool = route.params.tool as string | undefined;

  const sourceItems = sourceStore.sourcesData.map((source) => {
    const tools = toolsForSource(source);
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
        active: source.id === currentSourceId && tool.id === currentTool,
        onSelect: () => {
          handleSourceToolSelect(source.id, tool.id);
          open.value = false;
        },
      })),
    };
  });

  return [
    [{ label: 'Sources', type: 'label' as const }, ...sourceItems],
    [
      { label: 'Settings', type: 'label' as const },
      {
        label: 'Sources',
        icon: 'i-lucide-layers',
        active: route.name === 'sources',
        onSelect: () => {
          goToSources();
          open.value = false;
        },
      },
      {
        label: 'System',
        icon: 'i-lucide-activity',
        active: route.name === 'system',
        onSelect: () => {
          router.push({ name: 'system' });
          open.value = false;
        },
      },
    ],
  ];
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
    {
      label: uiStore.textSize === 'compact' ? 'Comfortable text' : 'Compact text',
      icon: 'i-lucide-a-large-small',
      onSelect: () => {
        uiStore.toggleTextSize();
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
</script>

<template>
  <UDashboardSidebar
    id="default"
    v-model:open="open"
    collapsible
    resizable
    class="bg-muted"
    :ui="{ footer: 'lg:border-t lg:border-accented dark:lg:border-default' }"
  >
    <!-- Header: logo -->
    <template #header="{ collapsed }">
      <RouterLink
        to="/"
        class="flex items-center gap-2 px-2.5"
        :class="collapsed ? 'justify-center' : ''"
      >
        <UIcon name="i-lucide-layers" class="h-5 w-5 shrink-0 text-lg text-brand" />
        <span v-if="!collapsed" class="font-brand text-lg font-semibold text-brand"
          >Brightflow</span
        >
      </RouterLink>
    </template>

    <!-- Body: source tree -->
    <template #default="{ collapsed }">
      <!-- Sources + Settings groups -->
      <UNavigationMenu
        :collapsed="collapsed"
        :items="navItems"
        orientation="vertical"
        highlight
        tooltip
        popover
      />
    </template>

    <!-- Footer: user menu -->
    <template #footer="{ collapsed }">
      <UDropdownMenu :items="userMenuItems" :content="{ side: 'right', align: 'end' }">
        <button
          class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-sm transition-colors hover:bg-elevated"
          :class="collapsed ? 'justify-center' : ''"
        >
          <UIcon name="i-lucide-user" class="h-4 w-4 shrink-0 text-muted" />
          <span v-if="!collapsed" class="truncate text-sm text-highlighted">
            {{ authStore.user?.displayName ?? 'User' }}
          </span>
        </button>
      </UDropdownMenu>
    </template>
  </UDashboardSidebar>
</template>

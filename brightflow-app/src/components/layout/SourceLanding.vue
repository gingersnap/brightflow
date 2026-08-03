<script setup lang="ts">
/**
 * Sources overview: a card grid of every source plus a dashed
 * "Add Source" tile. Selecting a card routes to that source's settings
 * tool rather than a data view.
 */

import { Plus } from '@lucide/vue';
import { useRouter } from 'vue-router';

import SourceCard from '@/components/layout/SourceCard.vue';
import { useSources } from '@/composables/useSources';
import type { UnifiedSource } from '@/types';

const router = useRouter();

const { sources, isPending } = useSources();

function handleSelect(id: string): void {
  router.push({ name: 'source-tool', params: { sourceId: id, tool: 'settings' } });
}
</script>

<template>
  <UDashboardPanel id="sources">
    <template #header>
      <UDashboardNavbar title="Sources">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div v-if="isPending" class="flex flex-1 items-center justify-center p-4">
        <p class="text-muted">Loading sources...</p>
      </div>

      <div v-else class="grid grid-cols-1 gap-4 p-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        <SourceCard
          v-for="source in sources"
          :key="source.id"
          :source="source"
          @select="handleSelect"
        />

        <!-- Add Source card -->
        <button
          class="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-default p-4 text-muted transition-all hover:border-primary-500/50 hover:text-highlighted"
          @click="router.push({ name: 'sources-new' })"
        >
          <Plus class="h-6 w-6" />
          <span class="text-sm font-medium">Add Source</span>
        </button>
      </div>
    </template>
  </UDashboardPanel>
</template>

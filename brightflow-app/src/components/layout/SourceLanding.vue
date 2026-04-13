<script setup lang="ts">
import { useQuery } from '@pinia/colada';
import { Plus } from 'lucide-vue-next';

import SourceCard from '@/components/layout/SourceCard.vue';
import { sourceApi } from '@/services/api';
import { useSourceStore } from '@/stores/source';
import { useUiStore } from '@/stores/ui';
import type { UnifiedSource } from '@/types';

const sourceStore = useSourceStore();
const uiStore = useUiStore();

const { data: sources, isPending } = useQuery({
  key: ['unified-sources'],
  query: async () => {
    const result = await sourceApi.unifiedList();
    const data = result ?? ([] as UnifiedSource[]);
    sourceStore.setSourcesData(data);
    return data;
  },
});

function handleSelect(id: string): void {
  sourceStore.selectSource(id);
}
</script>

<template>
  <UDashboardPanel id="sources">
    <template #header>
      <UDashboardNavbar title="Sources" />
    </template>

    <template #body>
      <div v-if="isPending" class="flex flex-1 items-center justify-center p-6">
        <p class="text-muted">Loading sources...</p>
      </div>

      <div v-else class="grid grid-cols-1 gap-4 p-6 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        <SourceCard
          v-for="source in sources"
          :key="source.id"
          :source="source"
          @select="handleSelect"
        />

        <!-- Add Source card -->
        <button
          class="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-default p-5 text-muted transition-all hover:border-primary-500/50 hover:text-highlighted"
          @click="uiStore.setShowConnect(true)"
        >
          <Plus class="h-6 w-6" />
          <span class="text-sm font-medium">Add Source</span>
        </button>
      </div>
    </template>
  </UDashboardPanel>
</template>
